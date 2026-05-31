use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::str::FromStr;

use crate::search::TaskSearch;

pub type TaskId = Option<u64>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub uuid: String,
    pub core: CoreTaskFields,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreTaskFields {
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub target_date: Option<NaiveDate>,
    pub deadline: Option<NaiveDate>,
    pub launch_date: Option<NaiveDate>,
    pub target_time_hint: Option<String>,
    pub deadline_time_hint: Option<String>,
    pub launch_time_hint: Option<String>,
    pub project: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct NewTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub target_date: Option<NaiveDate>,
    pub deadline: Option<NaiveDate>,
    pub launch_date: Option<NaiveDate>,
    pub target_time_hint: Option<String>,
    pub deadline_time_hint: Option<String>,
    pub launch_time_hint: Option<String>,
    pub project: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub target_date: Option<NaiveDate>,
    pub clear_target_date: bool,
    pub deadline: Option<NaiveDate>,
    pub clear_deadline: bool,
    pub launch_date: Option<NaiveDate>,
    pub clear_launch_date: bool,
    pub target_time_hint: Option<String>,
    pub clear_target_time_hint: bool,
    pub deadline_time_hint: Option<String>,
    pub clear_deadline_time_hint: bool,
    pub launch_time_hint: Option<String>,
    pub clear_launch_time_hint: bool,
    pub project: Option<String>,
    pub clear_project: bool,
    pub tags: Option<Vec<String>>,
    pub clear_tags: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Unstarted,
    Active,
    Waiting,
    Suspended,
    Done,
    Abandoned,
    Mistaken,
    Duplicated,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unstarted => "unstarted",
            Self::Active => "active",
            Self::Waiting => "waiting",
            Self::Suspended => "suspended",
            Self::Done => "done",
            Self::Abandoned => "abandoned",
            Self::Mistaken => "mistaken",
            Self::Duplicated => "duplicated",
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str((*self).as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "unstarted" => Ok(Self::Unstarted),
            "active" => Ok(Self::Active),
            "waiting" => Ok(Self::Waiting),
            "pending" | "suspended" => Ok(Self::Suspended),
            "done" => Ok(Self::Done),
            "abandoned" => Ok(Self::Abandoned),
            "mistaken" => Ok(Self::Mistaken),
            "duplicated" => Ok(Self::Duplicated),
            _ => Err(format!(
                "invalid status `{value}`; expected one of: unstarted, active, waiting, suspended, done, abandoned, mistaken, duplicated"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub created_at: DateTime<Utc>,
    pub kind: AnnotationKind,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    #[default]
    Note,
    Progress,
    Decision,
    Handover,
}

impl AnnotationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Progress => "progress",
            Self::Decision => "decision",
            Self::Handover => "handover",
        }
    }
}

impl fmt::Display for AnnotationKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str((*self).as_str())
    }
}

impl FromStr for AnnotationKind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "note" => Ok(Self::Note),
            "progress" => Ok(Self::Progress),
            "decision" => Ok(Self::Decision),
            "handover" => Ok(Self::Handover),
            _ => Err(format!(
                "invalid annotation kind `{value}`; expected one of: note, progress, decision, handover"
            )),
        }
    }
}

impl Task {
    pub fn new(id: TaskId, uuid: String, title: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            uuid,
            core: CoreTaskFields {
                title,
                description: None,
                status: TaskStatus::Unstarted,
                created_at: now,
                updated_at: now,
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
        }
    }

    pub fn id_text(&self) -> String {
        self.id
            .map(|id| id.to_string())
            .unwrap_or_else(|| self.uuid.clone())
    }

    pub fn title(&self) -> &str {
        &self.core.title
    }

    pub fn urgency(&self) -> f64 {
        self.extra
            .get("urgency")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    }
}

pub fn set_extra_path(extra: &mut Map<String, Value>, key: &str, value: Value) {
    extra.remove(key);

    let mut parts = key.split('.').peekable();
    let mut current = extra;

    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_string(), value);
            return;
        }

        let entry = current
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry
            .as_object_mut()
            .expect("extra path intermediate values should be objects");
    }
}

pub fn get_extra_path<'a>(extra: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(value) = extra.get(key) {
        return Some(value);
    }

    let mut parts = key.split('.');
    let first = parts.next()?;
    let mut current = extra.get(first)?;

    for part in parts {
        current = current.as_object()?.get(part)?;
    }

    Some(current)
}

pub fn unset_extra_path(extra: &mut Map<String, Value>, key: &str) -> bool {
    if extra.remove(key).is_some() {
        return true;
    }

    fn remove_recursive(current: &mut Map<String, Value>, parts: &[&str]) -> bool {
        if parts.is_empty() {
            return false;
        }

        if parts.len() == 1 {
            return current.remove(parts[0]).is_some();
        }

        let Some(child) = current.get_mut(parts[0]) else {
            return false;
        };
        let Some(child_map) = child.as_object_mut() else {
            return false;
        };

        let removed = remove_recursive(child_map, &parts[1..]);
        if removed && child_map.is_empty() {
            current.remove(parts[0]);
        }
        removed
    }

    let parts = key.split('.').collect::<Vec<_>>();
    remove_recursive(extra, &parts)
}

#[async_trait]
pub trait TaskBackend {
    async fn list_pending(&self) -> Result<Vec<Task>>;
    async fn list_all(&self) -> Result<Vec<Task>>;
    async fn search(&self, query: &TaskSearch) -> Result<Vec<Task>>;
    async fn add(&self, input: NewTaskInput) -> Result<Task>;
    async fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task>;
    async fn get_task(&self, id: u64) -> Result<Task>;
    async fn add_annotation(&self, id: u64, kind: AnnotationKind, body: String) -> Result<Task>;
    async fn set_status(&self, id: u64, status: TaskStatus) -> Result<Task>;
    async fn set_extra(&self, id: u64, key: &str, value: Value) -> Result<Task>;
    async fn get_extra(&self, id: u64, key: &str) -> Result<Option<Value>>;
    async fn unset_extra(&self, id: u64, key: &str) -> Result<Task>;
    async fn mark_done(&self, id: u64) -> Result<Task>;
    async fn mark_abandoned(&self, id: u64) -> Result<Task>;
    async fn mark_mistaken(&self, id: u64) -> Result<Task>;
    async fn mark_duplicated(&self, id: u64) -> Result<Task>;
    async fn next_task(&self) -> Result<Option<Task>>;
}
