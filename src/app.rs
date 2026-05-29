use anyhow::Result;

use crate::backend::{Task, TaskBackend};
use crate::cli::{Cli, Commands};
use crate::config::AppConfig;
use crate::local_backend::LocalBackend;

pub async fn run(cli: Cli) -> Result<()> {
    let config = AppConfig::load()?;
    let client = LocalBackend::new(config.resolve_sqlite_path()?)?;

    match cli.command {
        Commands::List => {
            let tasks = client.list_pending()?;
            print_tasks(&tasks);
        }
        Commands::Add { description } => {
            let task = client.add(&description)?;
            println!("added {}: {}", task.id_text(), task.title());
        }
        Commands::Edit { id, description } => {
            let task = client.edit(id, &description)?;
            println!("updated {}: {}", task.id_text(), task.title());
        }
        Commands::Delete { id } => {
            let task = client.delete(id)?;
            println!("deleted {}: {}", task.id_text(), task.title());
        }
        Commands::Done { id } => {
            let task = client.mark_done(id)?;
            println!("done {}: {}", task.id_text(), task.title());
        }
        Commands::Next => match client.next_task()? {
            Some(task) => println!("next {}: {}", task.id_text(), task.title()),
            None => println!("no pending tasks"),
        },
        Commands::Serve => {
            crate::web::serve(client, config.server.resolve()?).await?;
        }
    }

    Ok(())
}

fn print_tasks(tasks: &[Task]) {
    if tasks.is_empty() {
        println!("no pending tasks");
        return;
    }

    for task in tasks {
        println!("{} {}", task.id_text(), task.title());
    }
}
