use std::fs;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use rustls::{ClientConfig, RootCertStore};
use serde_json::{Map, Value};
use tokio_postgres::{Client, Row};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::backend::{NewTaskInput, Task, TaskBackend, TaskStatus, UpdateTaskInput};
use crate::search::{TaskSearch, compile_postgres};

#[derive(Debug, Clone)]
pub struct PostgresBackend {
    client: Arc<Client>,
}

impl PostgresBackend {
    pub async fn connect(connection_url: &str, ssl_root_cert: Option<&Path>) -> Result<Self> {
        let tls = MakeRustlsConnect::new(build_client_config(ssl_root_cert)?);
        let redacted_url = redact_connection_url(connection_url);
        let (client, connection) = tokio_postgres::connect(connection_url, tls)
            .await
            .with_context(|| format!("failed to connect to Postgres at {redacted_url}"))?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                eprintln!("postgres connection error: {error}");
            }
        });

        let backend = Self {
            client: Arc::new(client),
        };
        backend.init_schema().await?;
        Ok(backend)
    }

    async fn init_schema(&self) -> Result<()> {
        self.client
            .batch_execute(
                r#"
                CREATE TABLE IF NOT EXISTS task_statuses (
                  id BIGINT PRIMARY KEY,
                  name TEXT NOT NULL UNIQUE
                );

                CREATE TABLE IF NOT EXISTS tasks (
                  id BIGSERIAL PRIMARY KEY,
                  uuid TEXT NOT NULL UNIQUE,
                  title TEXT NOT NULL,
                  description TEXT,
                  status_id BIGINT NOT NULL REFERENCES task_statuses(id),
                  created_at TIMESTAMPTZ NOT NULL,
                  updated_at TIMESTAMPTZ NOT NULL,
                  target_date DATE,
                  deadline DATE,
                  launch_date DATE,
                  target_time_hint TEXT,
                  deadline_time_hint TEXT,
                  launch_time_hint TEXT,
                  project TEXT,
                  tags_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                  extra_json JSONB NOT NULL DEFAULT '{}'::jsonb
                );

                CREATE TABLE IF NOT EXISTS task_annotations (
                  id BIGSERIAL PRIMARY KEY,
                  task_id BIGINT NOT NULL REFERENCES tasks(id),
                  created_at TIMESTAMPTZ NOT NULL,
                  kind TEXT NOT NULL,
                  body TEXT NOT NULL
                );
                "#,
            )
            .await?;

        seed_task_statuses(&self.client).await
    }

    async fn fetch_task(&self, id: u64) -> Result<Task> {
        let row = self
            .client
            .query_opt(TASK_SELECT_SQL, &[&(id as i64)])
            .await?;

        let row = row.ok_or_else(|| anyhow!("task {id} was not found"))?;
        map_task_row(&row)
    }
}

#[async_trait]
impl TaskBackend for PostgresBackend {
    async fn list_pending(&self) -> Result<Vec<Task>> {
        let rows = self.client.query(TASK_LIST_PENDING_SQL, &[]).await?;
        rows.iter().map(map_task_row).collect()
    }

    async fn search(&self, query: &TaskSearch) -> Result<Vec<Task>> {
        let expr = query.parse()?;
        let compiled = compile_postgres(expr.as_ref())?;
        let params = compiled
            .params
            .iter()
            .map(|value| value.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect::<Vec<_>>();
        let rows = self.client.query(&compiled.sql, &params).await?;
        rows.iter().map(map_task_row).collect()
    }

    async fn add(&self, input: NewTaskInput) -> Result<Task> {
        let mut task = Task::new(None, generate_postgres_uuid(), input.title);
        task.core.description = input.description;
        task.core.status = input.status;
        task.core.target_date = input.target_date;
        task.core.deadline = input.deadline;
        task.core.launch_date = input.launch_date;
        task.core.target_time_hint = input.target_time_hint;
        task.core.deadline_time_hint = input.deadline_time_hint;
        task.core.launch_time_hint = input.launch_time_hint;
        task.core.project = input.project;
        task.core.tags = input.tags;

        let tags_json = serde_json::to_value(&task.core.tags)?;
        let extra_json = serde_json::to_value(&task.extra)?;
        let row = self
            .client
            .query_one(
                r#"
                INSERT INTO tasks (
                  uuid,
                  title,
                  description,
                  status_id,
                  created_at,
                  updated_at,
                  target_date,
                  deadline,
                  launch_date,
                  target_time_hint,
                  deadline_time_hint,
                  launch_time_hint,
                  project,
                  tags_json,
                  extra_json
                ) VALUES (
                  $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
                )
                RETURNING id
                "#,
                &[
                    &task.uuid,
                    &task.core.title,
                    &task.core.description,
                    &task_status_id(task.core.status),
                    &task.core.created_at,
                    &task.core.updated_at,
                    &task.core.target_date,
                    &task.core.deadline,
                    &task.core.launch_date,
                    &task.core.target_time_hint,
                    &task.core.deadline_time_hint,
                    &task.core.launch_time_hint,
                    &task.core.project,
                    &tags_json,
                    &extra_json,
                ],
            )
            .await?;

        task.id = Some(row.get::<_, i64>(0) as u64);
        Ok(task)
    }

    async fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task> {
        let mut task = self.fetch_task(id).await?;

        if let Some(title) = input.title {
            task.core.title = title;
        }
        if let Some(description) = input.description {
            task.core.description = Some(description);
        }
        if input.clear_target_date {
            task.core.target_date = None;
        }
        if let Some(target_date) = input.target_date {
            task.core.target_date = Some(target_date);
        }
        if input.clear_deadline {
            task.core.deadline = None;
        }
        if let Some(deadline) = input.deadline {
            task.core.deadline = Some(deadline);
        }
        if input.clear_launch_date {
            task.core.launch_date = None;
        }
        if let Some(launch_date) = input.launch_date {
            task.core.launch_date = Some(launch_date);
        }
        if input.clear_target_time_hint {
            task.core.target_time_hint = None;
        }
        if let Some(target_time_hint) = input.target_time_hint {
            task.core.target_time_hint = Some(target_time_hint);
        }
        if input.clear_deadline_time_hint {
            task.core.deadline_time_hint = None;
        }
        if let Some(deadline_time_hint) = input.deadline_time_hint {
            task.core.deadline_time_hint = Some(deadline_time_hint);
        }
        if input.clear_launch_time_hint {
            task.core.launch_time_hint = None;
        }
        if let Some(launch_time_hint) = input.launch_time_hint {
            task.core.launch_time_hint = Some(launch_time_hint);
        }
        if input.clear_project {
            task.core.project = None;
        }
        if let Some(project) = input.project {
            task.core.project = Some(project);
        }
        if input.clear_tags {
            task.core.tags = Vec::new();
        }
        if let Some(tags) = input.tags {
            task.core.tags = tags;
        }

        let tags_json = serde_json::to_value(&task.core.tags)?;
        let updated = self
            .client
            .execute(
                r#"
                UPDATE tasks
                SET title = $1,
                    description = $2,
                    status_id = $3,
                    updated_at = $4,
                    target_date = $5,
                    deadline = $6,
                    launch_date = $7,
                    target_time_hint = $8,
                    deadline_time_hint = $9,
                    launch_time_hint = $10,
                    project = $11,
                    tags_json = $12
                WHERE id = $13
                "#,
                &[
                    &task.core.title,
                    &task.core.description,
                    &task_status_id(task.core.status),
                    &Utc::now(),
                    &task.core.target_date,
                    &task.core.deadline,
                    &task.core.launch_date,
                    &task.core.target_time_hint,
                    &task.core.deadline_time_hint,
                    &task.core.launch_time_hint,
                    &task.core.project,
                    &tags_json,
                    &(id as i64),
                ],
            )
            .await?;

        if updated == 0 {
            return Err(anyhow!("task {id} was not found"));
        }

        self.fetch_task(id).await
    }

    async fn get_task(&self, id: u64) -> Result<Task> {
        self.fetch_task(id).await
    }

    async fn set_status(&self, id: u64, status: TaskStatus) -> Result<Task> {
        update_task_status(self, id, status).await
    }

    async fn set_extra(&self, id: u64, key: &str, value: Value) -> Result<Task> {
        let mut task = self.fetch_task(id).await?;
        task.extra.insert(key.to_string(), value);
        let extra_json = serde_json::to_value(&task.extra)?;

        self.client
            .execute(
                "UPDATE tasks SET extra_json = $1, updated_at = $2 WHERE id = $3",
                &[&extra_json, &Utc::now(), &(id as i64)],
            )
            .await?;

        self.fetch_task(id).await
    }

    async fn get_extra(&self, id: u64, key: &str) -> Result<Option<Value>> {
        let task = self.fetch_task(id).await?;
        Ok(task.extra.get(key).cloned())
    }

    async fn unset_extra(&self, id: u64, key: &str) -> Result<Task> {
        let mut task = self.fetch_task(id).await?;
        task.extra.remove(key);
        let extra_json = serde_json::to_value(&task.extra)?;

        self.client
            .execute(
                "UPDATE tasks SET extra_json = $1, updated_at = $2 WHERE id = $3",
                &[&extra_json, &Utc::now(), &(id as i64)],
            )
            .await?;

        self.fetch_task(id).await
    }

    async fn mark_done(&self, id: u64) -> Result<Task> {
        self.set_status(id, TaskStatus::Done).await
    }

    async fn mark_abandoned(&self, id: u64) -> Result<Task> {
        self.set_status(id, TaskStatus::Abandoned).await
    }

    async fn mark_mistaken(&self, id: u64) -> Result<Task> {
        self.set_status(id, TaskStatus::Mistaken).await
    }

    async fn mark_duplicated(&self, id: u64) -> Result<Task> {
        self.set_status(id, TaskStatus::Duplicated).await
    }

    async fn next_task(&self) -> Result<Option<Task>> {
        Ok(self.list_pending().await?.into_iter().next())
    }
}

async fn seed_task_statuses(client: &Client) -> Result<()> {
    let statuses = [
        (1_i64, "unstarted"),
        (2_i64, "active"),
        (3_i64, "suspended"),
        (4_i64, "done"),
        (5_i64, "abandoned"),
        (6_i64, "mistaken"),
        (7_i64, "duplicated"),
        (8_i64, "waiting"),
    ];

    for (id, name) in statuses {
        client
            .execute(
                r#"
                INSERT INTO task_statuses (id, name) VALUES ($1, $2)
                ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name
                "#,
                &[&id, &name],
            )
            .await?;
    }

    Ok(())
}

async fn update_task_status(
    backend: &PostgresBackend,
    id: u64,
    status: TaskStatus,
) -> Result<Task> {
    let updated = backend
        .client
        .execute(
            "UPDATE tasks SET status_id = $1, updated_at = $2 WHERE id = $3",
            &[&task_status_id(status), &Utc::now(), &(id as i64)],
        )
        .await?;

    if updated == 0 {
        return Err(anyhow!("task {id} was not found"));
    }

    backend.fetch_task(id).await
}

fn map_task_row(row: &Row) -> Result<Task> {
    let tags_json: Value = row.get("tags_json");
    let extra_json: Value = row.get("extra_json");
    let tags = serde_json::from_value(tags_json)?;
    let extra: Map<String, Value> = serde_json::from_value(extra_json)?;

    Ok(Task {
        id: Some(row.get::<_, i64>("id") as u64),
        uuid: row.get("uuid"),
        core: crate::backend::CoreTaskFields {
            title: row.get("title"),
            description: row.get("description"),
            status: parse_task_status(&row.get::<_, String>("status_name")),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            target_date: row.get("target_date"),
            deadline: row.get("deadline"),
            launch_date: row.get("launch_date"),
            target_time_hint: row.get("target_time_hint"),
            deadline_time_hint: row.get("deadline_time_hint"),
            launch_time_hint: row.get("launch_time_hint"),
            project: row.get("project"),
            tags,
        },
        annotations: Vec::new(),
        extra,
    })
}

fn task_status_id(status: TaskStatus) -> i64 {
    match status {
        TaskStatus::Unstarted => 1,
        TaskStatus::Active => 2,
        TaskStatus::Suspended => 3,
        TaskStatus::Done => 4,
        TaskStatus::Abandoned => 5,
        TaskStatus::Mistaken => 6,
        TaskStatus::Duplicated => 7,
        TaskStatus::Waiting => 8,
    }
}

fn parse_task_status(value: &str) -> TaskStatus {
    value.parse().unwrap_or(TaskStatus::Unstarted)
}

fn generate_postgres_uuid() -> String {
    format!(
        "pg-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

fn redact_connection_url(connection_url: &str) -> String {
    let scheme_end = match connection_url.find("://") {
        Some(index) => index + 3,
        None => return "<redacted-postgres-url>".to_string(),
    };

    let after_scheme = &connection_url[scheme_end..];
    let authority_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];

    let Some(at_index) = authority.rfind('@') else {
        return connection_url.to_string();
    };

    let host_part = &authority[at_index + 1..];
    format!(
        "{}<redacted>@{}{}",
        &connection_url[..scheme_end],
        host_part,
        &after_scheme[authority_end..]
    )
}

fn build_client_config(ssl_root_cert: Option<&Path>) -> Result<ClientConfig> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut root_store = RootCertStore::empty();
    if let Some(path) = ssl_root_cert {
        let certs = load_root_certificates(path)?;
        let (added, ignored) = root_store.add_parsable_certificates(certs);
        if added == 0 {
            return Err(anyhow!(
                "failed to load any CA certificates from {}",
                path.display()
            ));
        }
        if ignored > 0 {
            eprintln!(
                "ignored {ignored} malformed CA certificate(s) from {}",
                path.display()
            );
        }
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    Ok(config)
}

fn load_root_certificates(path: &Path) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let file = fs::File::open(path).with_context(|| {
        format!(
            "failed to open Postgres CA certificate at {}",
            path.display()
        )
    })?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to parse Postgres CA certificate at {}",
                path.display()
            )
        })
}

const TASK_SELECT_SQL: &str = r#"
SELECT
  tasks.id,
  tasks.uuid,
  tasks.title,
  tasks.description,
  task_statuses.name AS status_name,
  tasks.created_at,
  tasks.updated_at,
  tasks.target_date,
  tasks.deadline,
  tasks.launch_date,
  tasks.target_time_hint,
  tasks.deadline_time_hint,
  tasks.launch_time_hint,
  tasks.project,
  tasks.tags_json,
  tasks.extra_json
FROM tasks
JOIN task_statuses ON task_statuses.id = tasks.status_id
WHERE tasks.id = $1
"#;

const TASK_LIST_PENDING_SQL: &str = r#"
SELECT
  tasks.id,
  tasks.uuid,
  tasks.title,
  tasks.description,
  task_statuses.name AS status_name,
  tasks.created_at,
  tasks.updated_at,
  tasks.target_date,
  tasks.deadline,
  tasks.launch_date,
  tasks.target_time_hint,
  tasks.deadline_time_hint,
  tasks.launch_time_hint,
  tasks.project,
  tasks.tags_json,
  tasks.extra_json
FROM tasks
JOIN task_statuses ON task_statuses.id = tasks.status_id
WHERE task_statuses.name IN ('unstarted', 'active', 'waiting', 'suspended')
ORDER BY
  CASE task_statuses.name
    WHEN 'active' THEN 0
    WHEN 'unstarted' THEN 1
    WHEN 'waiting' THEN 2
    WHEN 'suspended' THEN 3
    ELSE 4
  END ASC,
  CASE WHEN deadline IS NULL THEN 1 ELSE 0 END,
  deadline ASC,
  CASE WHEN target_date IS NULL THEN 1 ELSE 0 END,
  target_date ASC,
  created_at ASC
"#;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{build_client_config, parse_task_status, redact_connection_url};
    use crate::backend::TaskStatus;

    #[test]
    fn maps_pending_to_suspended_for_compatibility() {
        assert_eq!(parse_task_status("pending"), TaskStatus::Suspended);
    }

    #[test]
    fn redacts_passwords_in_connection_urls() {
        assert_eq!(
            redact_connection_url(
                "postgresql://postgres:secret@db.example.com:5432/postgres?sslmode=require"
            ),
            "postgresql://<redacted>@db.example.com:5432/postgres?sslmode=require"
        );
    }

    #[test]
    fn reports_invalid_ca_certificate_files() {
        let path = unique_temp_path("taskforce-postgres-root");
        fs::write(&path, "not a cert").expect("write pem");

        let error = build_client_config(Some(&path)).expect_err("invalid cert should fail");
        assert!(
            error
                .to_string()
                .contains("failed to load any CA certificates")
        );

        fs::remove_file(path).expect("remove pem");
    }

    fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}.pem"))
    }
}
