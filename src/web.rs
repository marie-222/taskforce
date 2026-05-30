use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router, http::StatusCode};
use serde_json::{Map, Value, json};

use crate::backend::{Task, TaskBackend};
use crate::i18n::tr;
use crate::plugin::plugin_manifests;

pub async fn serve<B>(backend: B, addr: SocketAddr) -> Result<()>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    println!("serving taskforce at http://{local_addr}");
    axum::serve(listener, app_router(backend)).await?;
    Ok(())
}

pub fn app_router<B>(backend: B) -> Router
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(index))
        .route("/assets/index.css", get(index_css_asset))
        .route("/assets/index.js", get(index_js_asset))
        .route("/assets/task_detail.css", get(task_detail_css_asset))
        .route("/assets/task_detail.js", get(task_detail_js_asset))
        .route("/api/tasks", get(api_tasks::<B>))
        .route("/api/tasks/{id}", get(api_task::<B>))
        .route("/api/plugin-manifests", get(api_plugin_manifests))
        .route("/tasks/{id}", get(task_detail))
        .with_state(backend)
}

async fn index() -> Html<String> {
    Html(render_index_html())
}

async fn api_tasks<B>(State(backend): State<B>) -> Result<Json<Vec<Task>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .list_pending()
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_task<B>(
    Path(id): Path<u64>,
    State(backend): State<B>,
) -> Result<Json<Task>, StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .get_task(id)
        .await
        .map(Json)
        .map_err(map_task_error_status)
}

async fn api_plugin_manifests() -> Result<Json<Value>, StatusCode> {
    plugin_fields_value()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn task_detail(Path(_id): Path<u64>) -> Html<String> {
    Html(render_detail_html())
}

async fn index_js_asset() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../assets/index.js"),
    )
}

async fn index_css_asset() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/index.css"),
    )
}

async fn task_detail_js_asset() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "application/javascript; charset=utf-8")],
        include_str!("../assets/task_detail.js"),
    )
}

async fn task_detail_css_asset() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/task_detail.css"),
    )
}

fn map_task_error_status(error: anyhow::Error) -> StatusCode {
    if error.to_string().contains("not found") {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

fn render_index_html() -> String {
    render_template(
        INDEX_HTML_TEMPLATE,
        &[
            ("__LOCAL_TASK_CONSOLE__", tr("Local Task Console")),
            (
                "__INDEX_LEDE__",
                tr(
                    "Open tasks from your local taskforce database, served over a tiny local HTTP view.",
                ),
            ),
            ("__OPEN_TASKS__", tr("Open Tasks")),
            ("__REFRESH__", tr("Refresh")),
            ("__NO_OPEN_TASKS__", tr("No open tasks.")),
            ("__INDEX_CONFIG_JSON__", index_config_json()),
        ],
    )
}

fn render_detail_html() -> String {
    render_template(
        DETAIL_HTML_TEMPLATE,
        &[
            ("__BACK_TO_OPEN_TASKS__", tr("Back to Open Tasks")),
            ("__LOADING_TASK__", tr("Loading task…")),
            ("__DESCRIPTION__", tr("Description")),
            ("__ORIGINAL_REQUEST__", tr("Description")),
            ("__SCHEDULE__", tr("Schedule")),
            ("__PROJECT_AND_TAGS__", tr("Project & Tags")),
            ("__PROJECT__", tr("Project")),
            ("__TAGS__", tr("Tags")),
            ("__DETAIL_CONFIG_JSON__", detail_config_json()),
        ],
    )
}

fn render_template(template: &str, replacements: &[(&str, String)]) -> String {
    let mut rendered = template.to_string();
    for (placeholder, replacement) in replacements {
        rendered = rendered.replace(placeholder, replacement);
    }
    rendered
}

fn plugin_fields_value() -> Result<Value> {
    let mut plugins = Map::new();

    for manifest in plugin_manifests()? {
        let mut fields = Map::new();
        for field in &manifest.custom_fields {
            fields.insert(
                field.path.clone(),
                json!({
                    "label": tr(&field.label),
                    "placement": field.placement,
                }),
            );
        }

        plugins.insert(
            manifest.id.clone(),
            json!({
                "name": tr(&manifest.name),
                "fields": fields,
            }),
        );
    }

    Ok(Value::Object(plugins))
}

fn index_config_json() -> String {
    json!({
        "labels": {
            "urgency": tr("urgency"),
            "no_open_tasks": tr("No open tasks."),
        }
    })
    .to_string()
}

fn detail_config_json() -> String {
    json!({
        "labels": {
            "dash": tr("—"),
            "annotations": tr("annotations"),
            "target": tr("Target"),
            "deadline": tr("Deadline"),
            "launch": tr("Launch"),
            "tree": tr("Tree"),
            "raw": tr("Raw"),
            "expand_all": tr("Expand all"),
            "collapse_all": tr("Collapse all"),
        },
        "messages": {
            "no_abstract_yet": tr("No abstract yet."),
            "no_description_yet": tr("No description yet."),
            "task_not_found": tr("Task not found"),
            "task_could_not_be_loaded": tr("The requested task could not be loaded."),
            "no_project": tr("no project"),
            "no_tags": tr("No tags."),
            "no_extra_data": tr("No extra data."),
            "no_original_request": tr("Original request text is not available."),
        },
        "status_labels": {
            "unstarted": tr("unstarted"),
            "active": tr("active"),
            "waiting": tr("waiting"),
            "suspended": tr("suspended"),
            "done": tr("done"),
            "abandoned": tr("abandoned"),
            "mistaken": tr("mistaken"),
            "duplicated": tr("duplicated"),
        },
    })
    .to_string()
}

const INDEX_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>taskforce</title>
    <link rel="stylesheet" href="/assets/index.css" />
  </head>
  <body>
    <main>
      <section class="hero">
        <div class="eyebrow">__LOCAL_TASK_CONSOLE__</div>
        <h1>taskforce</h1>
        <p class="lede">
          __INDEX_LEDE__
        </p>
      </section>
      <section class="panel">
        <div class="panel-head">
          <h2>__OPEN_TASKS__</h2>
          <button id="refresh" type="button">__REFRESH__</button>
        </div>
        <ul id="task-list"></ul>
        <div id="empty" class="empty" hidden>__NO_OPEN_TASKS__</div>
      </section>
    </main>
    <script id="taskforce-index-config" type="application/json">__INDEX_CONFIG_JSON__</script>
    <script src="/assets/index.js"></script>
  </body>
</html>
"#;

const DETAIL_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>taskforce task</title>
    <link rel="stylesheet" href="/assets/task_detail.css" />
  </head>
  <body>
    <main>
      <div class="topline">
        <a class="backlink" href="/">__BACK_TO_OPEN_TASKS__</a>
      </div>
      <section class="shell">
        <header class="hero">
          <div class="meta-line" id="meta-line"></div>
          <h1 id="task-title">__LOADING_TASK__</h1>
          <p class="hero-copy" id="task-abstract"></p>
        </header>
        <div class="grid">
          <section class="column">
            <div class="section" id="task-description-section" hidden>
              <div class="section-label">__DESCRIPTION__</div>
              <p class="section-body" id="task-description"></p>
            </div>
            <div class="section">
              <div class="section-label">__ORIGINAL_REQUEST__</div>
              <div class="section-body" id="task-original-request"></div>
            </div>
          </section>
          <aside class="column">
            <div class="section">
              <div class="section-label">__SCHEDULE__</div>
              <div class="schedule" id="schedule"></div>
            </div>
            <div class="section">
              <div class="section-label">__PROJECT_AND_TAGS__</div>
              <div class="kv-list">
                <div class="kv-item">
                  <div class="kv-key">__PROJECT__</div>
                  <div class="kv-value" id="project-value"></div>
                </div>
                <div class="kv-item">
                  <div class="kv-key">__TAGS__</div>
                  <div class="tag-list" id="tag-list"></div>
                </div>
              </div>
            </div>
            <div class="section">
              <div id="plugin-extra-sections" class="kv-list"></div>
            </div>
          </aside>
        </div>
      </section>
    </main>
    <script id="taskforce-detail-config" type="application/json">__DETAIL_CONFIG_JSON__</script>
    <script src="/assets/task_detail.js"></script>
  </body>
</html>
"#;

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use serde_json::{Map, Value};
    use tower::ServiceExt;

    use crate::backend::{
        CoreTaskFields, NewTaskInput, Task, TaskBackend, TaskStatus, UpdateTaskInput,
    };
    use crate::search::TaskSearch;

    #[derive(Clone)]
    struct MockBackend {
        tasks: Vec<Task>,
    }

    #[async_trait]
    impl TaskBackend for MockBackend {
        async fn list_pending(&self) -> anyhow::Result<Vec<Task>> {
            Ok(self.tasks.clone())
        }

        async fn search(&self, _query: &TaskSearch) -> anyhow::Result<Vec<Task>> {
            Ok(self.tasks.clone())
        }

        async fn add(&self, _input: NewTaskInput) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn edit(&self, _id: u64, _input: UpdateTaskInput) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn get_task(&self, id: u64) -> anyhow::Result<Task> {
            self.tasks
                .iter()
                .find(|task| task.id == Some(id))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("task {id} was not found"))
        }

        async fn set_status(&self, _id: u64, _status: TaskStatus) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn set_extra(
            &self,
            _id: u64,
            _key: &str,
            _value: serde_json::Value,
        ) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn get_extra(
            &self,
            _id: u64,
            _key: &str,
        ) -> anyhow::Result<Option<serde_json::Value>> {
            unreachable!("not used in web tests")
        }

        async fn unset_extra(&self, _id: u64, _key: &str) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn mark_done(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn mark_abandoned(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn mark_mistaken(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn mark_duplicated(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        async fn next_task(&self) -> anyhow::Result<Option<Task>> {
            Ok(self.tasks.first().cloned())
        }
    }

    #[tokio::test]
    async fn api_tasks_returns_task_json() {
        let backend = MockBackend {
            tasks: vec![Task {
                id: Some(3),
                uuid: "abc".into(),
                core: CoreTaskFields {
                    title: "Ship MVP".into(),
                    description: None,
                    status: TaskStatus::Unstarted,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    target_date: None,
                    deadline: None,
                    launch_date: None,
                    target_time_hint: None,
                    deadline_time_hint: None,
                    launch_time_hint: None,
                    project: None,
                    tags: Vec::new(),
                },
                annotations: Vec::new(),
                extra: Map::from_iter([
                    ("urgency".into(), Value::from(7.5)),
                    (
                        "chatwork".into(),
                        Value::Object(Map::from_iter([
                            ("requester".into(), Value::String("佐藤".into())),
                            ("abstract".into(), Value::String("Batch fix".into())),
                        ])),
                    ),
                ]),
            }],
        };
        let app = crate::web::app_router(backend);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tasks")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("\"title\":\"Ship MVP\""));
    }

    #[tokio::test]
    async fn index_page_renders_taskforce_heading() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(text.contains("taskforce"));
        assert!(text.contains("Open Tasks"));
        assert!(text.contains("/assets/index.css"));
        assert!(text.contains("/assets/index.js"));
        assert!(text.contains("taskforce-index-config"));
    }

    #[tokio::test]
    async fn detail_page_and_task_api_render_structured_task_data() {
        let backend = MockBackend {
            tasks: vec![Task {
                id: Some(7),
                uuid: "detail".into(),
                core: CoreTaskFields {
                    title: "Review scope output".into(),
                    description: Some("Core description".into()),
                    status: TaskStatus::Active,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    target_date: None,
                    deadline: None,
                    launch_date: None,
                    target_time_hint: None,
                    deadline_time_hint: None,
                    launch_time_hint: None,
                    project: Some("taskforce".into()),
                    tags: vec!["chatwork".into()],
                },
                annotations: Vec::new(),
                extra: Map::from_iter([(
                    "chatwork".into(),
                    Value::Object(Map::from_iter([
                        ("requester".into(), Value::String("佐藤".into())),
                        (
                            "description".into(),
                            Value::String("Show the full details.".into()),
                        ),
                    ])),
                )]),
            }],
        };
        let app = crate::web::app_router(backend);

        let api_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tasks/7")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(api_response.status(), StatusCode::OK);
        let api_body = to_bytes(api_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let api_text = String::from_utf8(api_body.to_vec()).expect("utf8");
        assert!(api_text.contains("\"title\":\"Review scope output\""));

        let page_response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/7")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(page_response.status(), StatusCode::OK);
        let page_body = to_bytes(page_response.into_body(), usize::MAX)
            .await
            .expect("body");
        let page_text = String::from_utf8(page_body.to_vec()).expect("utf8");
        assert!(page_text.contains("Description"));
        assert!(page_text.contains("Back to Open Tasks"));
        assert!(page_text.contains("/assets/task_detail.css"));
        assert!(page_text.contains("/assets/task_detail.js"));
        assert!(page_text.contains("taskforce-detail-config"));
    }

    #[tokio::test]
    async fn detail_page_uses_only_core_description_for_description_section() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("function effectiveDescription(task) {"));
        assert!(text.contains("return task.core.description;"));
        assert!(text.contains("effectiveDescription(task)"));
    }

    #[tokio::test]
    async fn detail_page_hides_description_section_without_effective_description() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains(
            "const descriptionSection = document.getElementById(\"task-description-section\");"
        ));
        assert!(text.contains("descriptionSection.hidden = true;"));
    }

    #[tokio::test]
    async fn detail_styles_hide_hidden_sections() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.css")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("[hidden] {"));
        assert!(text.contains("display: none !important;"));
    }

    #[tokio::test]
    async fn detail_page_uses_plugin_sections_for_extra_data() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("plugin-extra-sections"));
        assert!(!text.contains("requester-value"));
        assert!(!text.contains("request-url-value"));
        assert!(!text.contains("related-request-url-value"));
        assert!(!text.contains("source-value"));
    }

    #[tokio::test]
    async fn plugin_manifests_api_returns_translated_field_metadata() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/plugin-manifests")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let value: Value = serde_json::from_slice(&body).expect("json");

        assert_eq!(
            value["chatwork"]["fields"]["render_blocks"]["placement"],
            Value::String("left".into())
        );
        assert_eq!(
            value["chatwork"]["fields"]["description"]["placement"],
            Value::String("hidden".into())
        );
        assert_eq!(
            value["chatwork"]["fields"]["summary"]["label"],
            Value::String("Request Summary".into())
        );
    }

    #[tokio::test]
    async fn detail_page_normalizes_legacy_chatwork_extra_and_flattens_plugin_root() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("const legacyChatworkKeys = ["));
        assert!(text.contains("let pluginFields = {};"));
        assert!(text.contains("function normalizeChatworkExtra(extra) {"));
        assert!(text.contains("return extra.chatwork;"));
        assert!(text.contains("function pluginManifest(pluginKey) {"));
        assert!(text.contains("function pluginFieldMeta(path) {"));
        assert!(text.contains("function fieldPlacement(pluginKey, fieldKey) {"));
        assert!(text.contains(
            "if (pluginValue && typeof pluginValue === \"object\" && !Array.isArray(pluginValue))"
        ));
        assert!(text.contains("const entries = Object.entries(pluginValue);"));
        assert!(text.contains("renderJsonTree(`${pluginKey}.${childKey}`, childKey, childValue)"));
    }

    #[tokio::test]
    async fn detail_page_renders_plugin_sections_as_accordions() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("plugin-section"));
        assert!(text.contains("plugin-section__summary"));
        assert!(text.contains("plugin-section__content"));
        assert!(text.contains("details.open = true;"));
    }

    #[tokio::test]
    async fn detail_page_uses_legacy_chatwork_extra_for_original_request_and_hides_source_tree() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("const chatwork = normalizeChatworkExtra(task.extra) ?? {};"));
        assert!(text.contains("fetch(\"/api/plugin-manifests\")"));
        assert!(text.contains("return fieldPlacement(pluginKey, fieldKey) === \"right\";"));
        assert!(text.contains("return parseChatworkRenderBlocks(source.body_raw);"));
    }

    #[tokio::test]
    async fn detail_page_prefers_render_blocks_for_original_request_and_hides_them_from_tree() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains(
            "const renderBlocks = leftFieldValue(\"chatwork\", chatwork, \"render_blocks\");"
        ));
        assert!(text.contains("renderOriginalRequest("));
    }

    #[tokio::test]
    async fn detail_page_links_external_urls_in_metadata_and_request_body() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("function isUrlFieldPath(path) {"));
        assert!(text.contains("function appendLinkifiedText(container, text) {"));
        assert!(text.contains("link.target = \"_blank\";"));
        assert!(text.contains("link.rel = \"noopener noreferrer\";"));
    }

    #[tokio::test]
    async fn detail_page_supports_quote_and_rule_render_blocks() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("if (rest.startsWith(\"[qt]\")) {"));
        assert!(text.contains("if (rest.startsWith(\"[hr]\")) {"));
        assert!(text.contains("kind: \"quote\""));
        assert!(text.contains("kind: \"rule\""));
        assert!(text.contains("block.kind === \"quote\""));
        assert!(text.contains("block.kind === \"rule\""));
    }

    #[tokio::test]
    async fn detail_page_supports_nested_children_inside_quote_blocks() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/task_detail.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let text = String::from_utf8(body.to_vec()).expect("utf8");

        assert!(text.contains("children: [],"));
        assert!(text.contains("if (Array.isArray(block.children) && block.children.length > 0) {"));
        assert!(text.contains("quote.appendChild(children);"));
        assert!(text.contains("children.className = \"request-block__children\";"));
    }
}
