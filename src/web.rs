use std::net::SocketAddr;

use anyhow::Result;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};

use crate::backend::{Task, TaskBackend};

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
        .route("/api/tasks", get(api_tasks::<B>))
        .with_state(backend)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn api_tasks<B>(State(backend): State<B>) -> Result<Json<Vec<Task>>, axum::http::StatusCode>
where
    B: TaskBackend + Clone + Send + Sync + 'static,
{
    backend
        .list_pending()
        .map(Json)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>taskforce</title>
    <style>
      :root {
        --bg: #f4efe7;
        --ink: #1a1816;
        --muted: #6c625a;
        --card: rgba(255, 252, 246, 0.84);
        --line: rgba(26, 24, 22, 0.12);
        --accent: #c4532f;
        --accent-2: #21453d;
      }

      * {
        box-sizing: border-box;
      }

      body {
        margin: 0;
        min-height: 100vh;
        font-family: "Iowan Old Style", "Palatino Linotype", serif;
        color: var(--ink);
        background:
          radial-gradient(circle at top left, rgba(196, 83, 47, 0.16), transparent 28%),
          radial-gradient(circle at bottom right, rgba(33, 69, 61, 0.18), transparent 32%),
          linear-gradient(145deg, #efe4d2 0%, var(--bg) 52%, #e5ddd1 100%);
      }

      main {
        width: min(980px, calc(100% - 32px));
        margin: 0 auto;
        padding: 48px 0 72px;
      }

      .hero {
        display: grid;
        gap: 14px;
        margin-bottom: 28px;
      }

      .eyebrow {
        letter-spacing: 0.18em;
        text-transform: uppercase;
        font-size: 12px;
        color: var(--accent-2);
      }

      h1 {
        margin: 0;
        font-size: clamp(42px, 8vw, 78px);
        line-height: 0.92;
        font-weight: 700;
      }

      .lede {
        max-width: 640px;
        margin: 0;
        font-size: 18px;
        line-height: 1.6;
        color: var(--muted);
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 24px;
        background: var(--card);
        backdrop-filter: blur(14px);
        box-shadow: 0 22px 60px rgba(26, 24, 22, 0.08);
        overflow: hidden;
      }

      .panel-head {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 16px;
        padding: 20px 22px;
        border-bottom: 1px solid var(--line);
      }

      .panel-head h2 {
        margin: 0;
        font-size: 24px;
      }

      button {
        border: 0;
        border-radius: 999px;
        padding: 10px 16px;
        font: inherit;
        color: #fffaf3;
        background: linear-gradient(135deg, var(--accent), #d47936);
        cursor: pointer;
      }

      ul {
        list-style: none;
        margin: 0;
        padding: 10px 0;
      }

      li {
        display: grid;
        grid-template-columns: auto 1fr auto;
        gap: 14px;
        align-items: center;
        padding: 14px 22px;
        border-top: 1px solid rgba(26, 24, 22, 0.08);
      }

      li:first-child {
        border-top: 0;
      }

      .task-id {
        min-width: 40px;
        font-size: 12px;
        letter-spacing: 0.1em;
        text-transform: uppercase;
        color: var(--accent-2);
      }

      .task-desc {
        font-size: 18px;
      }

      .task-urgency {
        color: var(--muted);
        font-size: 14px;
      }

      .empty {
        padding: 24px 22px 28px;
        color: var(--muted);
      }

      @media (max-width: 640px) {
        li {
          grid-template-columns: 1fr;
          gap: 6px;
        }
      }
    </style>
  </head>
  <body>
    <main>
      <section class="hero">
        <div class="eyebrow">Local Task Console</div>
        <h1>taskforce</h1>
        <p class="lede">
          Open tasks from your local taskforce database, served over a tiny local HTTP view.
        </p>
      </section>
      <section class="panel">
        <div class="panel-head">
          <h2>Open Tasks</h2>
          <button id="refresh" type="button">Refresh</button>
        </div>
        <ul id="task-list"></ul>
        <div id="empty" class="empty" hidden>No open tasks.</div>
      </section>
    </main>
    <script>
      const taskList = document.getElementById("task-list");
      const emptyState = document.getElementById("empty");
      const refreshButton = document.getElementById("refresh");

      async function loadTasks() {
        const response = await fetch("/api/tasks");
        const tasks = await response.json();

        taskList.innerHTML = "";
        emptyState.hidden = tasks.length !== 0;

        for (const task of tasks) {
          const item = document.createElement("li");
          item.innerHTML = `
            <span class="task-id">#${task.id ?? "?"}</span>
            <span class="task-desc"></span>
            <span class="task-urgency">urgency ${Number(task.extra?.urgency ?? 0).toFixed(1)}</span>
          `;
          item.querySelector(".task-desc").textContent = task.core.title;
          taskList.appendChild(item);
        }
      }

      refreshButton.addEventListener("click", () => {
        loadTasks().catch(console.error);
      });

      loadTasks().catch(console.error);
    </script>
  </body>
</html>
"#;

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use tower::ServiceExt;

    use crate::backend::{
        CoreTaskFields, NewTaskInput, Task, TaskBackend, TaskStatus, UpdateTaskInput,
    };

    #[derive(Clone)]
    struct MockBackend {
        tasks: Vec<Task>,
    }

    impl TaskBackend for MockBackend {
        fn list_pending(&self) -> anyhow::Result<Vec<Task>> {
            Ok(self.tasks.clone())
        }

        fn add(&self, _input: NewTaskInput) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn edit(&self, _id: u64, _input: UpdateTaskInput) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn get_task(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn set_extra(
            &self,
            _id: u64,
            _key: &str,
            _value: serde_json::Value,
        ) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn get_extra(&self, _id: u64, _key: &str) -> anyhow::Result<Option<serde_json::Value>> {
            unreachable!("not used in web tests")
        }

        fn unset_extra(&self, _id: u64, _key: &str) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn mark_done(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn mark_abandoned(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn mark_mistaken(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn mark_duplicated(&self, _id: u64) -> anyhow::Result<Task> {
            unreachable!("not used in web tests")
        }

        fn next_task(&self) -> anyhow::Result<Option<Task>> {
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
                extra: serde_json::Map::from_iter([(
                    "urgency".into(),
                    serde_json::Value::from(7.5),
                )]),
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
    }
}
