use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router, http::StatusCode};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::backend::{TaskBackend, TaskStatus};
use crate::dto::{TaskDto, TaskListItemDto};
use crate::i18n::tr;
use crate::plugin::{plugin_manifests, tr_plugin};

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
        .route("/tasks/all", get(all_tasks))
        .route("/assets/index.css", get(index_css_asset))
        .route("/assets/index.js", get(index_js_asset))
        .route("/assets/task_detail.css", get(task_detail_css_asset))
        .route("/assets/task_detail.js", get(task_detail_js_asset))
        .route("/api/tasks", get(api_tasks::<B>))
        .route("/api/tasks/all", get(api_all_tasks::<B>))
        .route("/api/search", get(api_search::<B>))
        .route("/api/status/{status}/tasks", get(api_status_tasks::<B>))
        .route("/api/tags/{tag}/tasks", get(api_tag_tasks::<B>))
        .route("/api/tasks/{id}", get(api_task::<B>))
        .route("/api/plugin-manifests", get(api_plugin_manifests))
        .route("/tags/{tag}", get(tag_tasks))
        .route("/search", get(search_page))
        .route("/status/{status}", get(status_tasks))
        .route("/tasks/{id}", get(task_detail))
        .with_state(backend)
}

async fn index() -> Html<String> {
    Html(render_index_html())
}

async fn all_tasks() -> Html<String> {
    Html(render_all_tasks_html())
}

async fn api_tasks<B>(
    State(backend): State<B>,
    axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
) -> Result<Json<Vec<TaskListItemDto>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .list_pending()
        .await
        .map(|tasks| {
            Json(
                filter_tasks_by_query(tasks, params.q.as_deref())
                    .iter()
                    .map(TaskListItemDto::from)
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_all_tasks<B>(
    State(backend): State<B>,
    axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
) -> Result<Json<Vec<TaskListItemDto>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .list_all()
        .await
        .map(|tasks| {
            Json(
                filter_tasks_by_query(tasks, params.q.as_deref())
                    .iter()
                    .map(TaskListItemDto::from)
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_status_tasks<B>(
    Path(status): Path<String>,
    State(backend): State<B>,
    axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
) -> Result<Json<Vec<TaskListItemDto>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    let status = status
        .parse::<TaskStatus>()
        .map_err(|_| StatusCode::NOT_FOUND)?;
    backend
        .list_all()
        .await
        .map(|tasks| {
            Json(
                filter_tasks_by_query(tasks, params.q.as_deref())
                    .iter()
                    .filter(|task| task.core.status == status)
                    .map(TaskListItemDto::from)
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_search<B>(
    State(backend): State<B>,
    axum::extract::Query(params): axum::extract::Query<SearchQueryParams>,
) -> Result<Json<Vec<TaskListItemDto>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    let where_clauses = params
        .where_clause
        .as_deref()
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    backend
        .search(&crate::search::TaskSearch::new(where_clauses))
        .await
        .map(|tasks| {
            Json(
                filter_tasks_by_query(tasks, params.q.as_deref())
                    .iter()
                    .map(TaskListItemDto::from)
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_tag_tasks<B>(
    Path(tag): Path<String>,
    State(backend): State<B>,
    axum::extract::Query(params): axum::extract::Query<ListQueryParams>,
) -> Result<Json<Vec<TaskListItemDto>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .list_pending()
        .await
        .map(|tasks| {
            Json(
                filter_tasks_by_query(tasks, params.q.as_deref())
                    .iter()
                    .filter(|task| task.core.tags.iter().any(|candidate| candidate == &tag))
                    .map(TaskListItemDto::from)
                    .collect(),
            )
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_task<B>(
    Path(id): Path<u64>,
    State(backend): State<B>,
) -> Result<Json<TaskDto>, StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .get_task(id)
        .await
        .map(|task| Json(TaskDto::from(&task)))
        .map_err(map_task_error_status)
}

async fn api_plugin_manifests() -> Result<Json<Value>, StatusCode> {
    plugin_fields_value()
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct ListQueryParams {
    q: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchQueryParams {
    #[serde(rename = "where")]
    where_clause: Option<String>,
    q: Option<String>,
}

fn filter_tasks_by_query(
    tasks: Vec<crate::backend::Task>,
    query: Option<&str>,
) -> Vec<crate::backend::Task> {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return tasks;
    };

    let query = query.to_lowercase();
    tasks
        .into_iter()
        .filter(|task| task_matches_query(task, &query))
        .collect()
}

fn task_matches_query(task: &crate::backend::Task, query: &str) -> bool {
    let mut haystacks = vec![
        task.id_text(),
        task.core.title.clone(),
        task.core.status.to_string(),
        task.core.project.clone().unwrap_or_default(),
        task.core.description.clone().unwrap_or_default(),
    ];
    haystacks.extend(task.core.tags.iter().cloned());

    haystacks
        .into_iter()
        .any(|value| value.to_lowercase().contains(query))
}

async fn task_detail(Path(_id): Path<u64>) -> Html<String> {
    Html(render_detail_html())
}

async fn tag_tasks(Path(tag): Path<String>) -> Html<String> {
    Html(render_tag_index_html(&tag))
}

async fn status_tasks(Path(status): Path<String>) -> Result<Html<String>, StatusCode> {
    let status = status
        .parse::<TaskStatus>()
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Html(render_status_index_html(status)))
}

async fn search_page() -> Html<String> {
    Html(render_search_html())
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
    render_index_page(
        "/",
        "taskforce".to_string(),
        tr("Open Tasks"),
        "/api/tasks",
        tr("No open tasks."),
        &["active", "unstarted", "waiting", "suspended"],
        &["active"],
    )
}

fn render_all_tasks_html() -> String {
    render_index_page(
        "/",
        tr("Back to Open Tasks"),
        tr("All Tasks"),
        "/api/tasks/all",
        tr("No tasks."),
        &[
            "active",
            "unstarted",
            "waiting",
            "suspended",
            "done",
            "abandoned",
            "mistaken",
            "duplicated",
        ],
        &["active"],
    )
}

fn render_status_index_html(status: TaskStatus) -> String {
    let status_name = tr(status.as_str());
    render_index_page(
        "/",
        tr("Back to Open Tasks"),
        format!("{}: {}", tr("Status"), status_name),
        &format!("/api/status/{}/tasks", status.as_str()),
        format!("{} {}", tr("No tasks with status"), status_name),
        &[status.as_str()],
        &[status.as_str()],
    )
}

fn render_search_html() -> String {
    render_template(
        SEARCH_INDEX_HTML_TEMPLATE,
        &[
            ("__BACKLINK_HREF__", "/".to_string()),
            ("__BACKLINK_LABEL__", tr("Back to Open Tasks")),
            ("__PAGE_TITLE__", escape_html(&tr("Search"))),
            ("__PANEL_TITLE__", tr("Search Results")),
            ("__SEARCH__", tr("Search")),
            ("__REFRESH__", tr("Refresh")),
            ("__RUN_SEARCH__", tr("Run Search")),
            ("__SEARCH_HINT__", tr("One WHERE clause per line")),
            (
                "__INDEX_CONFIG_JSON__",
                search_config_json(
                    "/api/search",
                    &[
                        "active",
                        "unstarted",
                        "waiting",
                        "suspended",
                        "done",
                        "abandoned",
                        "mistaken",
                        "duplicated",
                    ],
                ),
            ),
            ("__INDEX_CSS_URL__", asset_url("/assets/index.css")),
            ("__INDEX_JS_URL__", asset_url("/assets/index.js")),
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
            (
                "__TASK_DETAIL_CSS_URL__",
                asset_url("/assets/task_detail.css"),
            ),
            (
                "__TASK_DETAIL_JS_URL__",
                asset_url("/assets/task_detail.js"),
            ),
        ],
    )
}

fn render_tag_index_html(tag: &str) -> String {
    render_index_page(
        "/",
        tr("Back to Open Tasks"),
        format!("{}: #{tag}", tr("Tag")),
        &format!("/api/tags/{}/tasks", encode_path_segment(tag)),
        tr("No open tasks with this tag."),
        &["active", "unstarted", "waiting", "suspended"],
        &["active"],
    )
}

fn render_index_page(
    backlink_href: &str,
    backlink_label: String,
    title: String,
    api_url: &str,
    empty_message: String,
    status_order: &[&str],
    open_statuses: &[&str],
) -> String {
    render_template(
        INDEX_HTML_TEMPLATE,
        &[
            ("__BACKLINK_HREF__", backlink_href.to_string()),
            ("__BACKLINK_LABEL__", backlink_label),
            ("__PAGE_TITLE__", escape_html(&title)),
            ("__PANEL_TITLE__", title),
            ("__SEARCH__", tr("Search")),
            ("__REFRESH__", tr("Refresh")),
            (
                "__INDEX_CONFIG_JSON__",
                index_config_json_for_api(api_url, &empty_message, status_order, open_statuses),
            ),
            ("__INDEX_CSS_URL__", asset_url("/assets/index.css")),
            ("__INDEX_JS_URL__", asset_url("/assets/index.js")),
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

fn asset_url(path: &str) -> String {
    format!("{path}?v={}", asset_version())
}

fn asset_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    })
}

fn plugin_fields_value() -> Result<Value> {
    let mut plugins = Map::new();

    for manifest in plugin_manifests()? {
        let mut fields = Map::new();
        for field in &manifest.custom_fields {
            fields.insert(
                field.path.clone(),
                json!({
                    "label": tr_plugin(&manifest, &field.label),
                    "placement": field.placement,
                }),
            );
        }

        plugins.insert(
            manifest.id.clone(),
            json!({
                "name": tr_plugin(&manifest, &manifest.name),
                "group": manifest.group.as_ref().map(|group| json!({
                    "id": group.id,
                    "label": tr_plugin(&manifest, &group.label),
                })),
                "fields": fields,
            }),
        );
    }

    Ok(Value::Object(plugins))
}

fn index_config_json_for_api(
    api_url: &str,
    no_open_tasks: &str,
    status_order: &[&str],
    open_statuses: &[&str],
) -> String {
    json!({
        "api_url": api_url,
        "status_order": status_order,
        "open_statuses": open_statuses,
        "labels": {
            "urgency": tr("urgency"),
            "no_open_tasks": no_open_tasks,
            "no_filtered_tasks": tr("No matching tasks in this list."),
            "status": tr("Status"),
            "deadline": tr("Deadline"),
            "target": tr("Target"),
            "launch": tr("Launch"),
            "no_deadline": tr("No deadline"),
            "tasks": tr("tasks"),
            "all_tasks": tr("All Tasks"),
            "filter": tr("Filter"),
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

fn search_config_json(search_api_base: &str, status_order: &[&str]) -> String {
    json!({
        "search_api_base": search_api_base,
        "status_order": status_order,
        "open_statuses": status_order,
        "labels": {
            "urgency": tr("urgency"),
            "no_open_tasks": tr("No matching tasks."),
            "no_filtered_tasks": tr("No matching tasks in this list."),
            "status": tr("Status"),
            "deadline": tr("Deadline"),
            "target": tr("Target"),
            "launch": tr("Launch"),
            "no_deadline": tr("No deadline"),
            "tasks": tr("tasks"),
            "search_prompt": tr("Enter at least one WHERE clause."),
            "filter": tr("Filter"),
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

fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    encoded
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
    <link rel="stylesheet" href="__INDEX_CSS_URL__" />
  </head>
  <body>
    <main>
      <div class="topline">
        <a class="backlink" href="__BACKLINK_HREF__">__BACKLINK_LABEL__</a>
      </div>
      <section class="hero">
        <h1>__PAGE_TITLE__</h1>
      </section>
      <section class="panel">
        <div class="panel-head">
          <div class="panel-head-main">
            <h2>__PANEL_TITLE__</h2>
          </div>
          <button id="refresh" type="button">__REFRESH__</button>
          <div class="panel-head-search">
            <label class="quick-filter">
              <span class="quick-filter-icon" aria-hidden="true">⌕</span>
              <input id="quick-filter" class="quick-filter-input" type="search" aria-label="__SEARCH__" />
            </label>
          </div>
        </div>
        <ul id="task-list"></ul>
        <div id="empty" class="empty" hidden></div>
      </section>
    </main>
    <script id="taskforce-index-config" type="application/json">__INDEX_CONFIG_JSON__</script>
    <script src="__INDEX_JS_URL__"></script>
  </body>
</html>
"#;

const SEARCH_INDEX_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>taskforce search</title>
    <link rel="stylesheet" href="__INDEX_CSS_URL__" />
  </head>
  <body>
    <main>
      <div class="topline">
        <a class="backlink" href="__BACKLINK_HREF__">__BACKLINK_LABEL__</a>
      </div>
      <section class="hero">
        <h1>__PAGE_TITLE__</h1>
      </section>
      <section class="panel">
        <form id="search-form" class="search-form">
          <label class="search-form-label" for="search-where">__SEARCH__</label>
          <textarea id="search-where" class="search-textarea" name="where" placeholder="__SEARCH_HINT__"></textarea>
          <div class="search-actions">
            <button id="search-submit" type="submit">__RUN_SEARCH__</button>
          </div>
        </form>
      </section>
      <section class="panel">
        <div class="panel-head">
          <div class="panel-head-main">
            <h2>__PANEL_TITLE__</h2>
          </div>
          <button id="refresh" type="button">__REFRESH__</button>
          <div class="panel-head-search">
            <label class="quick-filter">
              <span class="quick-filter-icon" aria-hidden="true">⌕</span>
              <input id="quick-filter" class="quick-filter-input" type="search" aria-label="__SEARCH__" />
            </label>
          </div>
        </div>
        <ul id="task-list"></ul>
        <div id="empty" class="empty" hidden></div>
      </section>
    </main>
    <script id="taskforce-index-config" type="application/json">__INDEX_CONFIG_JSON__</script>
    <script src="__INDEX_JS_URL__"></script>
  </body>
</html>
"#;

const DETAIL_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>taskforce task</title>
    <link rel="stylesheet" href="__TASK_DETAIL_CSS_URL__" />
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
        </header>
        <div class="grid">
          <section class="column">
            <div class="section" id="task-description-section" hidden>
              <div class="section-label">__DESCRIPTION__</div>
              <p class="section-body" id="task-description"></p>
            </div>
            <div class="section" id="task-plugin-left-section" hidden>
              <div id="plugin-left-sections" class="kv-list"></div>
            </div>
          </section>
          <aside class="column">
            <div class="section" id="task-schedule-section">
              <div class="section-label">__SCHEDULE__</div>
              <div class="schedule" id="schedule"></div>
            </div>
            <div class="section" id="task-project-tags-section">
              <div class="section-label">__PROJECT_AND_TAGS__</div>
              <div class="kv-list">
                <div class="kv-item" id="task-project-row">
                  <div class="kv-key">__PROJECT__</div>
                  <div class="kv-value" id="project-value"></div>
                </div>
                <div class="kv-item" id="task-tags-row">
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
    <script src="__TASK_DETAIL_JS_URL__"></script>
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
        AnnotationKind, CoreTaskFields, NewTaskInput, Task, TaskBackend, TaskStatus,
        UpdateTaskInput,
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

        async fn list_all(&self) -> anyhow::Result<Vec<Task>> {
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

        async fn add_annotation(
            &self,
            _id: u64,
            _kind: AnnotationKind,
            _body: String,
        ) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
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
    async fn api_tag_tasks_returns_only_matching_open_tasks() {
        let first = Task {
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
                tags: vec!["release".into(), "ops".into()],
            },
            annotations: Vec::new(),
            extra: Map::new(),
        };
        let second = Task {
            id: Some(4),
            uuid: "def".into(),
            core: CoreTaskFields {
                title: "Review design".into(),
                description: None,
                status: TaskStatus::Active,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                target_date: None,
                deadline: None,
                launch_date: None,
                target_time_hint: None,
                deadline_time_hint: None,
                launch_time_hint: None,
                project: None,
                tags: vec!["design".into()],
            },
            annotations: Vec::new(),
            extra: Map::new(),
        };
        let app = crate::web::app_router(MockBackend {
            tasks: vec![first, second],
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/tags/release/tasks")
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
        assert!(!text.contains("\"title\":\"Review design\""));
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
        assert!(text.contains("/assets/index.css?v="));
        assert!(text.contains("/assets/index.js?v="));
        assert!(text.contains("taskforce-index-config"));
    }

    #[tokio::test]
    async fn tag_page_renders_tag_heading_and_backlink() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tags/release")
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
        assert!(text.contains("#release"));
        assert!(text.contains("Back to Open Tasks"));
        assert!(text.contains("/api/tags/release/tasks"));
    }

    #[tokio::test]
    async fn all_tasks_page_renders_backlink_and_all_tasks_api() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/all")
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
        assert!(text.contains("All Tasks"));
        assert!(text.contains("Back to Open Tasks"));
        assert!(text.contains("/api/tasks/all"));
    }

    #[tokio::test]
    async fn status_page_renders_backlink_and_status_api() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status/waiting")
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
        assert!(text.contains("Back to Open Tasks"));
        assert!(text.contains("/api/status/waiting/tasks"));
    }

    #[tokio::test]
    async fn search_page_renders_search_form_and_api() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/search")
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
        assert!(text.contains("search-form"));
        assert!(text.contains("search-where"));
        assert!(text.contains("/api/search"));
    }

    #[tokio::test]
    async fn api_search_uses_where_query_parameters() {
        let backend = MockBackend {
            tasks: vec![Task {
                id: Some(8),
                uuid: "search".into(),
                core: CoreTaskFields {
                    title: "Search Result".into(),
                    description: None,
                    status: TaskStatus::Waiting,
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
                extra: Map::new(),
            }],
        };
        let app = crate::web::app_router(backend);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/search?where=status%20=%20'waiting'")
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
        assert!(text.contains("\"title\":\"Search Result\""));
    }

    #[tokio::test]
    async fn index_page_assets_include_status_and_schedule_metadata() {
        let app = crate::web::app_router(MockBackend { tasks: Vec::new() });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/index.js")
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
        assert!(text.contains("statusLabel(status)"));
        assert!(text.contains("label(\"deadline\", \"Deadline\")"));
        assert!(text.contains("task-group-details"));
        assert!(text.contains("task-status--${status}"));
        assert!(text.contains("groupTasks(tasks)"));
        assert!(text.contains("task-group-chevron"));
        assert!(text.contains("task-meta-item--deadline"));
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
        assert!(page_text.contains("/assets/task_detail.css?v="));
        assert!(page_text.contains("/assets/task_detail.js?v="));
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
            Value::String("改修概要".into())
        );
    }

    #[tokio::test]
    async fn detail_page_filters_plugin_fields_by_manifest_placement() {
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

        assert!(text.contains("let pluginFields = {};"));
        assert!(text.contains("function pluginManifest(pluginKey) {"));
        assert!(text.contains("function pluginFieldMeta(path) {"));
        assert!(text.contains("function fieldPlacement(pluginKey, fieldKey) {"));
        assert!(
            text.contains(
                "function normalizePluginExtra(extra, placements = new Set([\"right\"])) {"
            )
        );
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
    async fn detail_page_ignores_plugins_without_manifests() {
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

        assert!(text.contains("fetch(\"/api/plugin-manifests\")"));
        assert!(text.contains(
            "return pluginManifest(pluginKey)?.fields?.[fieldKey]?.placement ?? \"hidden\";"
        ));
        assert!(text.contains("function hasFieldDescendants(pluginKey, fieldKey, placements) {"));
        assert!(text.contains("if (!pluginEnabled(key)) {"));
    }

    #[tokio::test]
    async fn detail_page_renders_left_and_right_plugin_sections_generically() {
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
            "const pluginLeftSection = document.getElementById(\"task-plugin-left-section\");"
        ));
        assert!(text.contains("renderPluginExtraSections("));
        assert!(text.contains("new Set([\"left\"])"));
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
    async fn detail_page_renders_generic_left_plugin_sections() {
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
            "const pluginLeftSections = document.getElementById(\"plugin-left-sections\");"
        ));
        assert!(text.contains("renderPluginExtraSections("));
        assert!(text.contains("{ showEmpty: false }"));
    }

    #[tokio::test]
    async fn detail_page_hides_missing_core_rows() {
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

        assert!(text.contains("projectRow.hidden = !task.core.project;"));
        assert!(text.contains("scheduleSection.hidden = scheduleCount === 0;"));
        assert!(text.contains("projectTagsSection.hidden = projectRow.hidden && tagsRow.hidden;"));
    }
}
