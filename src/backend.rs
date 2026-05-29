use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Option<u64>,
    pub uuid: String,
    pub description: String,
    #[serde(default)]
    pub urgency: f64,
}

impl Task {
    pub fn id_text(&self) -> String {
        self.id
            .map(|id| id.to_string())
            .unwrap_or_else(|| self.uuid.clone())
    }
}

pub trait TaskBackend {
    fn list_pending(&self) -> Result<Vec<Task>>;
    fn add(&self, description: &str) -> Result<Task>;
    fn edit(&self, id: u64, description: &str) -> Result<Task>;
    fn delete(&self, id: u64) -> Result<Task>;
    fn mark_done(&self, id: u64) -> Result<Task>;
    fn next_task(&self) -> Result<Option<Task>>;
}
