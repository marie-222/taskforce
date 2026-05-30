use anyhow::Result;
use chrono::NaiveDate;
use serde_json::Value;

use crate::backend::{NewTaskInput, Task, TaskBackend, UpdateTaskInput};
use crate::chatwork_plugin::import_chatwork_url;
use crate::cli::{Cli, Commands};
use crate::config::AppConfig;
use crate::db_backend::ConfiguredBackend;

pub async fn run(cli: Cli) -> Result<()> {
    let config = AppConfig::load()?;
    let client = ConfiguredBackend::open(&config).await?;

    match cli.command {
        Commands::List => {
            let tasks = client.list_pending().await?;
            print_tasks(&tasks);
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
        Commands::ImportChatwork { url } => {
            let task = import_chatwork_url(&client, &url).await?;
            println!("imported {}: {}", task.id_text(), task.title());
        }
        Commands::Next => match client.next_task().await? {
            Some(task) => println!("next {}: {}", task.id_text(), task.title()),
            None => println!("no open tasks"),
        },
        Commands::Serve => {
            crate::web::serve(client, config.server.resolve()?).await?;
        }
    }

    Ok(())
}

fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("no open tasks");
        return;
    }

    for task in tasks {
        println!("{} {}", task.id_text(), task.title());
    }
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
