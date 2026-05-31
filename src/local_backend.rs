use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::Map;

use crate::backend::{
    Annotation, AnnotationKind, NewTaskInput, Task, TaskBackend, TaskStatus, UpdateTaskInput,
    get_extra_path, set_extra_path, unset_extra_path,
};
use crate::search::{TaskSearch, compile_sqlite};

#[derive(Debug, Clone)]
pub struct LocalBackend {
    db_path: PathBuf,
}

impl LocalBackend {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let backend = Self { db_path };
        backend.init_schema()?;
        Ok(backend)
    }

    fn init_schema(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS task_statuses (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS tasks (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              uuid TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL,
              description TEXT,
              status_id INTEGER NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              target_date TEXT,
              deadline TEXT,
              launch_date TEXT,
              target_time_hint TEXT,
              deadline_time_hint TEXT,
              launch_time_hint TEXT,
              project TEXT,
              tags_json TEXT NOT NULL,
              extra_json TEXT NOT NULL,
              FOREIGN KEY(status_id) REFERENCES task_statuses(id)
            );

            CREATE TABLE IF NOT EXISTS task_annotations (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              task_id INTEGER NOT NULL,
              created_at TEXT NOT NULL,
              kind TEXT NOT NULL,
              body TEXT NOT NULL,
              FOREIGN KEY(task_id) REFERENCES tasks(id)
            );
            "#,
        )?;
        seed_task_statuses(&connection)?;
        migrate_legacy_status_column(&connection)?;
        migrate_description_column(&connection)?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))
    }

    fn fetch_task(&self, id: u64) -> Result<Task> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
              tasks.id,
              tasks.uuid,
              tasks.title,
              tasks.description,
              task_statuses.name,
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
            WHERE tasks.id = ?1
            "#,
        )?;

        let mut task = statement
            .query_row(params![id], map_task_row)
            .optional()?
            .ok_or_else(|| anyhow!("task {id} was not found"))?;
        task.annotations = fetch_annotations(&connection, id)?;
        Ok(task)
    }
}

#[async_trait]
impl TaskBackend for LocalBackend {
    async fn list_pending(&self) -> Result<Vec<Task>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
              tasks.id,
              tasks.uuid,
              tasks.title,
              tasks.description,
              task_statuses.name,
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
            "#,
        )?;

        let rows = statement.query_map([], map_task_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    async fn list_all(&self) -> Result<Vec<Task>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT
              tasks.id,
              tasks.uuid,
              tasks.title,
              tasks.description,
              task_statuses.name,
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
            ORDER BY
              CASE task_statuses.name
                WHEN 'active' THEN 0
                WHEN 'unstarted' THEN 1
                WHEN 'waiting' THEN 2
                WHEN 'suspended' THEN 3
                WHEN 'done' THEN 4
                WHEN 'abandoned' THEN 5
                WHEN 'mistaken' THEN 6
                WHEN 'duplicated' THEN 7
                ELSE 8
              END ASC,
              CASE WHEN deadline IS NULL THEN 1 ELSE 0 END,
              deadline ASC,
              CASE WHEN target_date IS NULL THEN 1 ELSE 0 END,
              target_date ASC,
              created_at ASC
            "#,
        )?;

        let rows = statement.query_map([], map_task_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    async fn search(&self, query: &TaskSearch) -> Result<Vec<Task>> {
        let expr = query.parse()?;
        let compiled = compile_sqlite(expr.as_ref())?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(&compiled.sql)?;
        let rows = statement.query_map(params_from_iter(compiled.params.iter()), map_task_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    async fn add(&self, input: NewTaskInput) -> Result<Task> {
        let mut task = Task::new(None, generate_local_uuid(), input.title);
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
        let connection = self.connection()?;
        let tags_json = serde_json::to_string(&task.core.tags)?;
        let extra_json = serde_json::to_string(&task.extra)?;

        connection.execute(
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
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                task.uuid,
                task.core.title,
                task.core.description,
                task_status_id(task.core.status),
                task.core.created_at.to_rfc3339(),
                task.core.updated_at.to_rfc3339(),
                task.core.target_date.map(|value| value.to_string()),
                task.core.deadline.map(|value| value.to_string()),
                task.core.launch_date.map(|value| value.to_string()),
                task.core.target_time_hint,
                task.core.deadline_time_hint,
                task.core.launch_time_hint,
                task.core.project,
                tags_json,
                extra_json,
            ],
        )?;

        task.id = Some(connection.last_insert_rowid() as u64);
        Ok(task)
    }

    async fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task> {
        let connection = self.connection()?;
        let now = Utc::now().to_rfc3339();
        let mut task = self.fetch_task(id)?;

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

        let tags_json = serde_json::to_string(&task.core.tags)?;
        let updated = connection.execute(
            r#"
            UPDATE tasks
            SET title = ?1,
                description = ?2,
                status_id = ?3,
                updated_at = ?4,
                target_date = ?5,
                deadline = ?6,
                launch_date = ?7,
                target_time_hint = ?8,
                deadline_time_hint = ?9,
                launch_time_hint = ?10,
                project = ?11,
                tags_json = ?12
            WHERE id = ?13
            "#,
            params![
                task.core.title,
                task.core.description,
                task_status_id(task.core.status),
                now,
                task.core.target_date.map(|value| value.to_string()),
                task.core.deadline.map(|value| value.to_string()),
                task.core.launch_date.map(|value| value.to_string()),
                task.core.target_time_hint,
                task.core.deadline_time_hint,
                task.core.launch_time_hint,
                task.core.project,
                tags_json,
                id,
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("task {id} was not found"));
        }

        self.fetch_task(id)
    }

    async fn get_task(&self, id: u64) -> Result<Task> {
        self.fetch_task(id)
    }

    async fn add_annotation(&self, id: u64, kind: AnnotationKind, body: String) -> Result<Task> {
        self.fetch_task(id)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO task_annotations (task_id, created_at, kind, body) VALUES (?1, ?2, ?3, ?4)",
            params![id, Utc::now().to_rfc3339(), kind.as_str(), body],
        )?;

        self.fetch_task(id)
    }

    async fn set_status(&self, id: u64, status: TaskStatus) -> Result<Task> {
        update_task_status(self, id, status)
    }

    async fn set_extra(&self, id: u64, key: &str, value: serde_json::Value) -> Result<Task> {
        let connection = self.connection()?;
        let mut task = self.fetch_task(id)?;
        set_extra_path(&mut task.extra, key, value);

        connection.execute(
            "UPDATE tasks SET extra_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&task.extra)?,
                Utc::now().to_rfc3339(),
                id
            ],
        )?;

        self.fetch_task(id)
    }

    async fn get_extra(&self, id: u64, key: &str) -> Result<Option<serde_json::Value>> {
        let task = self.fetch_task(id)?;
        Ok(get_extra_path(&task.extra, key).cloned())
    }

    async fn unset_extra(&self, id: u64, key: &str) -> Result<Task> {
        let connection = self.connection()?;
        let mut task = self.fetch_task(id)?;
        unset_extra_path(&mut task.extra, key);

        connection.execute(
            "UPDATE tasks SET extra_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&task.extra)?,
                Utc::now().to_rfc3339(),
                id
            ],
        )?;

        self.fetch_task(id)
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

fn update_task_status(backend: &LocalBackend, id: u64, status: TaskStatus) -> Result<Task> {
    let connection = backend.connection()?;
    let updated = connection.execute(
        "UPDATE tasks SET status_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![task_status_id(status), Utc::now().to_rfc3339(), id],
    )?;

    if updated == 0 {
        return Err(anyhow!("task {id} was not found"));
    }

    backend.fetch_task(id)
}

fn fetch_annotations(connection: &Connection, id: u64) -> Result<Vec<Annotation>> {
    let mut statement = connection.prepare(
        r#"
        SELECT created_at, kind, body
        FROM task_annotations
        WHERE task_id = ?1
        ORDER BY created_at ASC, id ASC
        "#,
    )?;
    let rows = statement.query_map(params![id], |row| {
        let created_at: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let body: String = row.get(2)?;
        Ok(Annotation {
            created_at: DateTime::parse_from_rfc3339(&created_at)
                .map(|value| value.with_timezone(&Utc))
                .map_err(decode_error)?,
            kind: parse_annotation_kind(&kind).map_err(|error| {
                decode_error(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
            })?,
            body,
        })
    })?;

    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let tags_json: String = row.get(14)?;
    let extra_json: String = row.get(15)?;

    let tags = serde_json::from_str(&tags_json).map_err(json_decode_error)?;
    let extra: Map<String, serde_json::Value> =
        serde_json::from_str(&extra_json).map_err(json_decode_error)?;

    Ok(Task {
        id: Some(row.get::<_, i64>(0)? as u64),
        uuid: row.get(1)?,
        core: crate::backend::CoreTaskFields {
            title: row.get(2)?,
            description: row.get(3)?,
            status: parse_task_status(&row.get::<_, String>(4)?),
            created_at: parse_datetime(&row.get::<_, String>(5)?)?,
            updated_at: parse_datetime(&row.get::<_, String>(6)?)?,
            target_date: parse_optional_date(row.get(7)?)?,
            deadline: parse_optional_date(row.get(8)?)?,
            launch_date: parse_optional_date(row.get(9)?)?,
            target_time_hint: row.get(10)?,
            deadline_time_hint: row.get(11)?,
            launch_time_hint: row.get(12)?,
            project: row.get(13)?,
            tags,
        },
        annotations: Vec::new(),
        extra,
    })
}

fn parse_annotation_kind(value: &str) -> std::result::Result<AnnotationKind, String> {
    value.parse()
}

fn parse_datetime(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(json_decode_error)
}

fn parse_optional_date(value: Option<String>) -> rusqlite::Result<Option<NaiveDate>> {
    value
        .map(|text| NaiveDate::parse_from_str(&text, "%Y-%m-%d").map_err(json_decode_error))
        .transpose()
}

fn json_decode_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn decode_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
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

fn seed_task_statuses(connection: &Connection) -> Result<()> {
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
        connection.execute(
            "INSERT INTO task_statuses (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name",
            params![id, name],
        )?;
    }

    Ok(())
}

fn migrate_legacy_status_column(connection: &Connection) -> Result<()> {
    let columns = connection
        .prepare("PRAGMA table_info(tasks)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let has_status_column = columns.iter().any(|column| column == "status");
    let has_status_id_column = columns.iter().any(|column| column == "status_id");

    if !has_status_id_column {
        connection.execute("ALTER TABLE tasks ADD COLUMN status_id INTEGER", [])?;
    }

    if !has_status_column {
        connection.execute(
            "UPDATE tasks SET status_id = COALESCE(status_id, 1) WHERE status_id IS NULL",
            [],
        )?;
        return Ok(());
    }

    connection.execute(
        "UPDATE tasks
         SET status_id = CASE status
            WHEN 'active' THEN 2
            WHEN 'pending' THEN 3
            WHEN 'suspended' THEN 3
            WHEN 'done' THEN 4
            WHEN 'abandoned' THEN 5
            WHEN 'mistaken' THEN 6
            WHEN 'duplicated' THEN 7
            ELSE 1
         END
         WHERE status_id IS NULL OR status_id = 0",
        [],
    )?;

    Ok(())
}

fn migrate_description_column(connection: &Connection) -> Result<()> {
    let columns = connection
        .prepare("PRAGMA table_info(tasks)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if columns.iter().any(|column| column == "description") {
        return Ok(());
    }

    connection.execute("ALTER TABLE tasks ADD COLUMN description TEXT", [])?;
    Ok(())
}

fn generate_local_uuid() -> String {
    format!(
        "local-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;

    use super::LocalBackend;
    use crate::backend::{NewTaskInput, TaskBackend, UpdateTaskInput};
    use crate::search::TaskSearch;

    #[tokio::test]
    async fn add_and_list_pending_tasks() -> Result<()> {
        let backend = LocalBackend::new(unique_db_path("taskforce-local-backend"))?;

        let added = backend
            .add(NewTaskInput {
                title: "Ship SQLite backend".into(),
                description: Some("Add a structured backend".into()),
                deadline: Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 5).expect("date")),
                tags: vec!["release".into()],
                ..Default::default()
            })
            .await?;
        let tasks = backend.list_pending().await?;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, added.id);
        assert_eq!(tasks[0].title(), "Ship SQLite backend");
        assert_eq!(
            tasks[0].core.description.as_deref(),
            Some("Add a structured backend")
        );
        assert_eq!(
            tasks[0].core.deadline,
            Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 5).expect("date"))
        );
        assert_eq!(tasks[0].core.tags, vec!["release"]);
        Ok(())
    }

    #[tokio::test]
    async fn edit_done_and_terminal_states() -> Result<()> {
        let backend = LocalBackend::new(unique_db_path("taskforce-local-backend-status"))?;

        let added = backend
            .add(NewTaskInput {
                title: "Old title".into(),
                ..Default::default()
            })
            .await?;
        let edited = backend
            .edit(
                added.id.expect("id"),
                UpdateTaskInput {
                    title: Some("New title".into()),
                    description: Some("Updated task description".into()),
                    project: Some("taskforce".into()),
                    tags: Some(vec!["ops".into()]),
                    ..Default::default()
                },
            )
            .await?;
        assert_eq!(edited.title(), "New title");
        assert_eq!(
            edited.core.description.as_deref(),
            Some("Updated task description")
        );
        assert_eq!(edited.core.project.as_deref(), Some("taskforce"));
        assert_eq!(edited.core.tags, vec!["ops"]);

        let done = backend.mark_done(added.id.expect("id")).await?;
        assert_eq!(done.core.status, crate::backend::TaskStatus::Done);

        let second = backend
            .add(NewTaskInput {
                title: "Mistaken task".into(),
                ..Default::default()
            })
            .await?;
        let mistaken = backend.mark_mistaken(second.id.expect("id")).await?;
        assert_eq!(mistaken.core.status, crate::backend::TaskStatus::Mistaken);

        let third = backend
            .add(NewTaskInput {
                title: "Duplicated task".into(),
                ..Default::default()
            })
            .await?;
        let duplicated = backend.mark_duplicated(third.id.expect("id")).await?;
        assert_eq!(
            duplicated.core.status,
            crate::backend::TaskStatus::Duplicated
        );

        let fourth = backend
            .add(NewTaskInput {
                title: "Abandoned task".into(),
                ..Default::default()
            })
            .await?;
        let abandoned = backend.mark_abandoned(fourth.id.expect("id")).await?;
        assert_eq!(abandoned.core.status, crate::backend::TaskStatus::Abandoned);
        Ok(())
    }

    #[tokio::test]
    async fn clears_optional_fields_and_updates_extra() -> Result<()> {
        let backend = LocalBackend::new(unique_db_path("taskforce-local-backend-extra"))?;

        let added = backend
            .add(NewTaskInput {
                title: "Structured task".into(),
                deadline: Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 5).expect("date")),
                project: Some("taskforce".into()),
                ..Default::default()
            })
            .await?;
        let id = added.id.expect("id");

        let edited = backend
            .edit(
                id,
                UpdateTaskInput {
                    clear_deadline: true,
                    clear_project: true,
                    ..Default::default()
                },
            )
            .await?;
        assert_eq!(edited.core.deadline, None);
        assert_eq!(edited.core.project, None);

        backend
            .set_extra(
                id,
                "git.working_branch",
                serde_json::Value::String("feature/tag-pages".into()),
            )
            .await?;
        assert_eq!(
            backend.get_extra(id, "git.working_branch").await?,
            Some(serde_json::Value::String("feature/tag-pages".into()))
        );

        let edited = backend.unset_extra(id, "git.working_branch").await?;
        assert!(!edited.extra.contains_key("git.working_branch"));
        assert!(
            edited
                .extra
                .get("git")
                .and_then(|value| value.as_object())
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn searches_tasks_with_core_and_plugin_filters() -> Result<()> {
        let backend = LocalBackend::new(unique_db_path("taskforce-local-backend-search"))?;

        let first = backend
            .add(NewTaskInput {
                title: "Banner update".into(),
                description: Some("Landing page hero banner".into()),
                deadline: Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 7).expect("date")),
                tags: vec!["ops".into()],
                ..Default::default()
            })
            .await?;
        backend
            .set_extra(
                first.id.expect("id"),
                "chatwork",
                serde_json::json!({ "requester": "石井" }),
            )
            .await?;

        let second = backend
            .add(NewTaskInput {
                title: "Design QA".into(),
                deadline: Some(chrono::NaiveDate::from_ymd_opt(2026, 6, 20).expect("date")),
                tags: vec!["design".into()],
                ..Default::default()
            })
            .await?;
        backend
            .set_extra(
                second.id.expect("id"),
                "chatwork",
                serde_json::json!({ "requester": "田中" }),
            )
            .await?;

        let search = TaskSearch::new(vec![
            "deadline <= '2026-06-10'".into(),
            "tag = 'ops'".into(),
            "chatwork.requester = '石井'".into(),
        ]);
        let tasks = backend.search(&search).await?;

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title(), "Banner update");
        Ok(())
    }

    fn unique_db_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}.db"))
    }
}
