// Copyright (c) 2026- Masaki Ishii
// Copyright (c) 2026- Small Gear Lab
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use chrono::NaiveDate;
use serde_json::Value;

use crate::backend::{Annotation, NewTaskInput, Task, TaskBackend, UpdateTaskInput};
use crate::cli::{Cli, Commands};
use crate::config::AppConfig;
use crate::db_backend::ConfiguredBackend;
use crate::dto::TaskDto;
use crate::search::TaskSearch;

pub async fn run(cli: Cli) -> Result<()> {
    let config = AppConfig::load()?;
    let client = ConfiguredBackend::open(&config).await?;

    match cli.command {
        Commands::List { json } => {
            let tasks = client.list_pending().await?;
            print_tasks(&tasks, json)?;
        }
        Commands::Search {
            where_clauses,
            json,
        } => {
            let tasks = client.search(&TaskSearch::new(where_clauses)).await?;
            print_tasks(&tasks, json)?;
        }
        Commands::Show { id, json } => {
            let task = client.get_task(id).await?;
            print_task_detail(&task, json)?;
        }
        Commands::Add {
            title,
            status,
            target_date,
            deadline,
            launch_date,
            target_time_hint,
            deadline_time_hint,
            launch_time_hint,
            project,
            tags,
        } => {
            let task = client
                .add(NewTaskInput {
                    title,
                    description: None,
                    status: status.unwrap_or_default(),
                    target_date: parse_optional_date(target_date)?,
                    deadline: parse_optional_date(deadline)?,
                    launch_date: parse_optional_date(launch_date)?,
                    target_time_hint,
                    deadline_time_hint,
                    launch_time_hint,
                    project,
                    tags,
                })
                .await?;
            println!("added {}: {}", task.id_text(), task.title());
        }
        Commands::Edit {
            id,
            title,
            target_date,
            clear_target_date,
            deadline,
            clear_deadline,
            launch_date,
            clear_launch_date,
            target_time_hint,
            clear_target_time_hint,
            deadline_time_hint,
            clear_deadline_time_hint,
            launch_time_hint,
            clear_launch_time_hint,
            project,
            clear_project,
            tags,
            clear_tags,
        } => {
            let task = client
                .edit(
                    id,
                    UpdateTaskInput {
                        title,
                        description: None,
                        target_date: parse_optional_date(target_date)?,
                        clear_target_date,
                        deadline: parse_optional_date(deadline)?,
                        clear_deadline,
                        launch_date: parse_optional_date(launch_date)?,
                        clear_launch_date,
                        target_time_hint,
                        clear_target_time_hint,
                        deadline_time_hint,
                        clear_deadline_time_hint,
                        launch_time_hint,
                        clear_launch_time_hint,
                        project,
                        clear_project,
                        tags: (!tags.is_empty()).then_some(tags),
                        clear_tags,
                    },
                )
                .await?;
            println!("updated {}: {}", task.id_text(), task.title());
        }
        Commands::Set {
            id,
            key,
            value,
            json,
        } => {
            let value = parse_extra_value(value, json)?;
            let task = client.set_extra(id, &key, value).await?;
            println!("updated {}: {}", task.id_text(), task.title());
        }
        Commands::Status { id, status } => {
            if let Some(status) = status {
                let task = client.set_status(id, status).await?;
                println!("status {}: {} -> {}", task.id_text(), task.title(), status);
            } else {
                let task = client.get_task(id).await?;
                println!("{} {}", task.id_text(), task.core.status);
            }
        }
        Commands::Note {
            id,
            body,
            kind,
            key,
        } => {
            let task = client.add_annotation(id, kind, body, key).await?;
            let added = task.annotations.last().expect("annotation just added");
            println!("noted {} [{}] {}", task.id_text(), added.kind, added.body);
        }
        Commands::NoteEdit {
            id,
            body,
            key,
            index,
        } => {
            let task = if let Some(k) = key {
                client.edit_annotation_by_key(id, &k, body).await?
            } else if let Some(n) = index {
                client.edit_annotation_by_index(id, n, body).await?
            } else {
                anyhow::bail!("note-edit requires --key or --index");
            };
            println!("updated {} annotation", task.id_text());
        }
        Commands::NoteDelete { id, key, index } => {
            let task = if let Some(k) = key {
                client.delete_annotation_by_key(id, &k).await?
            } else if let Some(n) = index {
                client.delete_annotation_by_index(id, n).await?
            } else {
                anyhow::bail!("note-delete requires --key or --index");
            };
            println!("deleted {} annotation", task.id_text());
        }
        Commands::Get { id, key } => match client.get_extra(id, &key).await? {
            Some(value) => println!("{}", serde_json::to_string_pretty(&value)?),
            None => println!("null"),
        },
        Commands::Unset { id, key } => {
            let task = client.unset_extra(id, &key).await?;
            println!("updated {}: {}", task.id_text(), task.title());
        }
        Commands::Done { id } => {
            let task = client.mark_done(id).await?;
            println!("done {}: {}", task.id_text(), task.title());
        }
        Commands::Next => match client.next_task().await? {
            Some(task) => println!("next {}: {}", task.id_text(), task.title()),
            None => println!("no open tasks"),
        },
        Commands::Serve => {
            crate::web::serve(client, config.server.resolve()?, config.list.clone()).await?;
        }
    }

    Ok(())
}

fn print_tasks(tasks: &[Task], as_json: bool) -> Result<()> {
    if as_json {
        let dto = tasks.iter().map(TaskDto::from).collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&dto)?);
        return Ok(());
    }

    if tasks.is_empty() {
        println!("no open tasks");
        return Ok(());
    }

    for task in tasks {
        println!("{} {}", task.id_text(), task.title());
    }

    Ok(())
}

fn print_task_detail(task: &Task, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(&TaskDto::from(task))?);
        return Ok(());
    }

    println!("id: {}", task.id_text());
    println!("title: {}", task.title());
    println!("status: {}", task.core.status);
    if let Some(project) = &task.core.project {
        println!("project: {project}");
    }
    if !task.core.tags.is_empty() {
        println!("tags: {}", task.core.tags.join(", "));
    }
    println!("created_at: {}", task.core.created_at.to_rfc3339());
    println!("updated_at: {}", task.core.updated_at.to_rfc3339());
    if let Some(target_date) = task.core.target_date {
        println!("target_date: {target_date}");
    }
    if let Some(deadline) = task.core.deadline {
        println!("deadline: {deadline}");
    }
    if let Some(launch_date) = task.core.launch_date {
        println!("launch_date: {launch_date}");
    }
    if let Some(target_time_hint) = &task.core.target_time_hint {
        println!("target_time_hint: {target_time_hint}");
    }
    if let Some(deadline_time_hint) = &task.core.deadline_time_hint {
        println!("deadline_time_hint: {deadline_time_hint}");
    }
    if let Some(launch_time_hint) = &task.core.launch_time_hint {
        println!("launch_time_hint: {launch_time_hint}");
    }
    if let Some(description) = &task.core.description {
        println!();
        println!("description:");
        println!("{description}");
    }
    if !task.annotations.is_empty() {
        println!();
        println!("annotations:");
        for annotation in &task.annotations {
            print_annotation(annotation);
        }
    }
    if !task.extra.is_empty() {
        println!();
        println!("extra:");
        println!("{}", serde_json::to_string_pretty(&task.extra)?);
    }
    Ok(())
}

fn print_annotation(annotation: &Annotation) {
    println!(
        "- [{}] {} {}",
        annotation.kind,
        annotation.created_at.to_rfc3339(),
        annotation.body
    );
}

fn parse_optional_date(value: Option<String>) -> Result<Option<NaiveDate>> {
    value
        .map(|text| {
            NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .map_err(|err| anyhow::anyhow!("invalid date `{text}`: {err}"))
        })
        .transpose()
}

fn parse_extra_value(value: String, json: bool) -> Result<Value> {
    if json {
        Ok(serde_json::from_str(&value)?)
    } else {
        Ok(Value::String(value))
    }
}
