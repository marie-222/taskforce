use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::backend::TaskStatus;
use crate::backend::{NewTaskInput, Task, TaskBackend, UpdateTaskInput};
use crate::config::{AppConfig, BackendKind};
use crate::local_backend::LocalBackend;
use crate::postgres_backend::PostgresBackend;

#[derive(Debug, Clone)]
pub enum ConfiguredBackend {
    Sqlite(LocalBackend),
    Postgres(PostgresBackend),
}

impl ConfiguredBackend {
    pub async fn open(config: &AppConfig) -> Result<Self> {
        match config.backend.kind {
            BackendKind::Sqlite => Ok(Self::Sqlite(LocalBackend::new(
                config.resolve_sqlite_path()?,
            )?)),
            BackendKind::Postgres => Ok(Self::Postgres(
                PostgresBackend::connect(
                    &config.resolve_postgres_url()?,
                    config.resolve_postgres_ssl_root_cert()?.as_deref(),
                )
                .await?,
            )),
        }
    }
}

#[async_trait]
impl TaskBackend for ConfiguredBackend {
    async fn list_pending(&self) -> Result<Vec<Task>> {
        match self {
            Self::Sqlite(backend) => backend.list_pending().await,
            Self::Postgres(backend) => backend.list_pending().await,
        }
    }

    async fn add(&self, input: NewTaskInput) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.add(input).await,
            Self::Postgres(backend) => backend.add(input).await,
        }
    }

    async fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.edit(id, input).await,
            Self::Postgres(backend) => backend.edit(id, input).await,
        }
    }

    async fn get_task(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.get_task(id).await,
            Self::Postgres(backend) => backend.get_task(id).await,
        }
    }

    async fn set_status(&self, id: u64, status: TaskStatus) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.set_status(id, status).await,
            Self::Postgres(backend) => backend.set_status(id, status).await,
        }
    }

    async fn set_extra(&self, id: u64, key: &str, value: Value) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.set_extra(id, key, value).await,
            Self::Postgres(backend) => backend.set_extra(id, key, value).await,
        }
    }

    async fn get_extra(&self, id: u64, key: &str) -> Result<Option<Value>> {
        match self {
            Self::Sqlite(backend) => backend.get_extra(id, key).await,
            Self::Postgres(backend) => backend.get_extra(id, key).await,
        }
    }

    async fn unset_extra(&self, id: u64, key: &str) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.unset_extra(id, key).await,
            Self::Postgres(backend) => backend.unset_extra(id, key).await,
        }
    }

    async fn mark_done(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_done(id).await,
            Self::Postgres(backend) => backend.mark_done(id).await,
        }
    }

    async fn mark_abandoned(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_abandoned(id).await,
            Self::Postgres(backend) => backend.mark_abandoned(id).await,
        }
    }

    async fn mark_mistaken(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_mistaken(id).await,
            Self::Postgres(backend) => backend.mark_mistaken(id).await,
        }
    }

    async fn mark_duplicated(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_duplicated(id).await,
            Self::Postgres(backend) => backend.mark_duplicated(id).await,
        }
    }

    async fn next_task(&self) -> Result<Option<Task>> {
        match self {
            Self::Sqlite(backend) => backend.next_task().await,
            Self::Postgres(backend) => backend.next_task().await,
        }
    }
}
