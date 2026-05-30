use anyhow::Result;
use serde_json::Value;

use crate::backend::{NewTaskInput, Task, TaskBackend, UpdateTaskInput};
use crate::config::{AppConfig, BackendKind};
use crate::local_backend::LocalBackend;

#[derive(Debug, Clone)]
pub enum ConfiguredBackend {
    Sqlite(LocalBackend),
}

impl ConfiguredBackend {
    pub fn open(config: &AppConfig) -> Result<Self> {
        match config.backend.kind {
            BackendKind::Sqlite => Ok(Self::Sqlite(LocalBackend::new(
                config.resolve_sqlite_path()?,
            )?)),
        }
    }
}

impl TaskBackend for ConfiguredBackend {
    fn list_pending(&self) -> Result<Vec<Task>> {
        match self {
            Self::Sqlite(backend) => backend.list_pending(),
        }
    }

    fn add(&self, input: NewTaskInput) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.add(input),
        }
    }

    fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.edit(id, input),
        }
    }

    fn get_task(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.get_task(id),
        }
    }

    fn set_extra(&self, id: u64, key: &str, value: Value) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.set_extra(id, key, value),
        }
    }

    fn get_extra(&self, id: u64, key: &str) -> Result<Option<Value>> {
        match self {
            Self::Sqlite(backend) => backend.get_extra(id, key),
        }
    }

    fn unset_extra(&self, id: u64, key: &str) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.unset_extra(id, key),
        }
    }

    fn mark_done(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_done(id),
        }
    }

    fn mark_abandoned(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_abandoned(id),
        }
    }

    fn mark_mistaken(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_mistaken(id),
        }
    }

    fn mark_duplicated(&self, id: u64) -> Result<Task> {
        match self {
            Self::Sqlite(backend) => backend.mark_duplicated(id),
        }
    }

    fn next_task(&self) -> Result<Option<Task>> {
        match self {
            Self::Sqlite(backend) => backend.next_task(),
        }
    }
}
