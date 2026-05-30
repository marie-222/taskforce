# Task Model

## Goal

`taskforce` should provide a task-oriented CLI and local workflow on top of its own structured storage.

The model should support both:

- a stable standard schema that the CLI and UI can rely on
- user-defined extensions for project-specific structured fields

## Design principles

- Keep the core task fields explicit and strongly typed.
- Allow arbitrary extra fields without schema migrations for every new workflow.
- Preserve a task-focused CLI UX where practical, without inheriting external storage constraints.
- Make backend replacement possible behind the existing `TaskBackend` abstraction.

## High-level model

Each task consists of:

- core fields
- annotations
- extra fields

Suggested shape:

```rust
pub struct Task {
    pub id: TaskId,
    pub core: CoreTaskFields,
    pub annotations: Vec<Annotation>,
    pub extra: serde_json::Map<String, serde_json::Value>,
}
```

## Core fields

These fields are part of the standard schema and may be assumed by CLI commands, filters, and the local web UI.

```rust
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
    pub tags: Vec<String>,
}
```

Initial `TaskStatus` candidates:

- `unstarted`
- `active`
- `suspended`
- `done`
- `abandoned`
- `mistaken`
- `duplicated`

Why these are core:

- `title` is the primary label in CLI and UI
- `description` gives the task model a plugin-independent standard body field
- `status` drives default listing behavior
- timestamps are necessary for sorting and auditability
- `target_date`, `deadline`, and `launch_date` match the user's real task template needs
- optional `*_time_hint` fields cover cases like `午前中` or `15:00まで` without forcing full timestamps everywhere
- `project` and `tags` support practical task grouping

## Annotations

Annotations are standard, structured child records rather than free text embedded in the main description.

```rust
pub struct Annotation {
    pub created_at: DateTime<Utc>,
    pub kind: AnnotationKind,
    pub body: String,
}
```

Initial `AnnotationKind` candidates:

- `note`
- `progress`
- `decision`
- `handover`

Annotations stay first-class because they are useful operationally, not because they mirror another tool's storage format.

## Extra fields

Extra fields are user-defined structured fields stored as JSON values.

In the long term, `extra` should be treated as the persisted merged result of extension outputs, not only as a bag of manually entered custom fields.

Examples:

- `requester`
- `chatwork_url`
- `room_id`
- `message_id`
- `target_sites`
- `summary`
- `purpose`
- `raw_request`

Rules:

- extra field keys must not collide with core field names
- values may be `string`, `number`, `bool`, `array`, `object`, or `null`
- CLI support should begin with string-oriented set/get flows, even if storage supports richer JSON
- importer- or plugin-like features may contribute structured fields before persistence, but the stored shape should still collapse into `extra`

## Extension direction

The system should eventually allow multiple extension producers to enrich a task before persistence.

Examples:

- a Chatwork importer contributes `requester`, `room_id`, `message_id`, `source_url`
- an urgency calculator contributes derived scheduling metadata
- a workflow-specific extension contributes `target_sites`, `purpose`, or deployment metadata

The important constraint is that these extension outputs should merge into the final `extra` payload instead of requiring every extension to add new top-level storage columns.

## Description and search direction

The task model should keep `core.description` as the standard description field even when plugins provide richer or source-specific descriptions.

Recommended direction:

- `core.description` is always available as the plugin-independent standard body field
- plugin-specific descriptions remain in `extra.<plugin_id>.description`
- UI rendering may prefer a plugin-provided description over `core.description`
- storage should keep both, rather than overwriting the core field destructively

Suggested display/search model:

- `effective_description = plugin override ?? core.description`
- default display uses `effective_description`
- default search also uses `effective_description`
- raw plugin descriptions remain available as provenance and may become opt-in secondary search targets later

Why this split is useful:

- the standard schema keeps a stable description field for generic tasks
- plugin imports can preserve source-specific wording without losing the app-level description contract
- future search can index a single canonical description field without double-counting both core and plugin text by default
- plugin-specific raw descriptions remain available for debugging, auditability, or advanced source-aware search later

## Storage direction

The preferred long-term backend is SQLite.

Recommended first schema:

- `tasks` table for core fields
- `task_annotations` table for annotations
- `extra_json` column on `tasks` for user-defined fields

Why this shape:

- core fields remain queryable and indexable
- annotations remain append-friendly and easy to filter
- user extensions do not force frequent migrations

Possible sketch:

```sql
CREATE TABLE tasks (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  status TEXT NOT NULL,
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
  extra_json TEXT NOT NULL
);

CREATE TABLE task_annotations (
  id INTEGER PRIMARY KEY,
  task_id INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  kind TEXT NOT NULL,
  body TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);
```

## CLI implications

The CLI should stay task-centric and compact where it helps:

- `add`
- `list`
- `done`
- `abandon`
- `mistake`
- `duplicate`
- `edit`
- `next`

Additional commands should support the structured model:

- `annotate`
- `set <id> <field> <value>`
- `get <id> <field>`
- `unset <id> <field>`

Expected behavior:

- core fields have dedicated flags or commands where appropriate
- extra fields are addressed through `set/get/unset`
- annotations are first-class instead of being pushed into `description`

## Migration direction

Near-term path:

1. Keep the current `TaskBackend` abstraction.
2. Select concrete database backends through config.
3. Use the local SQLite backend as the primary structured store.
3. Gradually move CLI and UI behavior to target the structured model first.

## Non-goals for the first local backend

- multi-user sync
- remote API design
- arbitrary schema validation language

The first goal is a local, structured, single-user backend with a compact task-oriented CLI surface.
