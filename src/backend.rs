use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use std::str::FromStr;


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

#[async_trait]
pub trait TaskBackend {
    async fn list_pending(&self) -> Result<Vec<Task>>;
    async fn add(&self, input: NewTaskInput) -> Result<Task>;
    async fn edit(&self, id: u64, input: UpdateTaskInput) -> Result<Task>;
    async fn get_task(&self, id: u64) -> Result<Task>;
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
