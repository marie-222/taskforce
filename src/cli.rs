use clap::{Parser, Subcommand};

use crate::backend::TaskStatus;

#[derive(Debug, Parser)]
#[command(name = "taskforce")]
#[command(about = "Local structured task workflow")]
pub struct Cli {
    #[arg(long, global = true)]
    pub env: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    List,
    Search {
        #[arg(long = "where")]
        where_clauses: Vec<String>,
    },
    Add {
        title: String,
        #[arg(long)]
        status: Option<TaskStatus>,
        #[arg(long)]
        target_date: Option<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long)]
        launch_date: Option<String>,
        #[arg(long)]
        target_time_hint: Option<String>,
        #[arg(long)]
        deadline_time_hint: Option<String>,
        #[arg(long)]
        launch_time_hint: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    Edit {
        id: u64,
        title: Option<String>,
        #[arg(long)]
        target_date: Option<String>,
        #[arg(long)]
        clear_target_date: bool,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(long)]
        clear_deadline: bool,
        #[arg(long)]
        launch_date: Option<String>,
        #[arg(long)]
        clear_launch_date: bool,
        #[arg(long)]
        target_time_hint: Option<String>,
        #[arg(long)]
        clear_target_time_hint: bool,
        #[arg(long)]
        deadline_time_hint: Option<String>,
        #[arg(long)]
        clear_deadline_time_hint: bool,
        #[arg(long)]
        launch_time_hint: Option<String>,
        #[arg(long)]
        clear_launch_time_hint: bool,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        clear_project: bool,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        clear_tags: bool,
    },
    Set {
        id: u64,
        key: String,
        value: String,
        #[arg(long)]
        json: bool,
    },
    Status {
        id: u64,
        status: Option<TaskStatus>,
    },
    Get {
        id: u64,
        key: String,
    },
    Unset {
        id: u64,
        key: String,
    },
    ImportChatwork {
        url: String,
    },
    Done {
        id: u64,
    },
    Next,
    Serve,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands, TaskStatus};

    #[test]
    fn parses_edit_command() {
        let cli = Cli::parse_from([
            "taskforce",
            "edit",
            "12",
            "Rewrite spec",
            "--deadline",
            "2026-06-05",
            "--tag",
            "ops",
        ]);

        match cli.command {
            Commands::Edit {
                id,
                title,
                deadline,
                clear_deadline,
                tags,
                ..
            } => {
                assert_eq!(id, 12);
                assert_eq!(title.as_deref(), Some("Rewrite spec"));
                assert_eq!(deadline.as_deref(), Some("2026-06-05"));
                assert!(!clear_deadline);
                assert_eq!(tags, vec!["ops"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_add_command_with_structured_flags() {
        let cli = Cli::parse_from([
            "taskforce",
            "add",
            "Rewrite spec",
            "--status",
            "waiting",
            "--target-date",
            "2026-06-02",
            "--deadline",
            "2026-06-05",
            "--project",
            "taskforce",
            "--tag",
            "ops",
            "--tag",
            "release",
        ]);

        match cli.command {
            Commands::Add {
                title,
                status,
                target_date,
                deadline,
                project,
                tags,
                ..
            } => {
                assert_eq!(title, "Rewrite spec");
                assert_eq!(status, Some(TaskStatus::Waiting));
                assert_eq!(target_date.as_deref(), Some("2026-06-02"));
                assert_eq!(deadline.as_deref(), Some("2026-06-05"));
                assert_eq!(project.as_deref(), Some("taskforce"));
                assert_eq!(tags, vec!["ops", "release"]);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_search_command() {
        let cli = Cli::parse_from([
            "taskforce",
            "search",
            "--where",
            "status = 'active'",
            "--where",
            "chatwork.requester = '石井'",
        ]);

        match cli.command {
            Commands::Search { where_clauses } => {
                assert_eq!(where_clauses.len(), 2);
                assert_eq!(where_clauses[0], "status = 'active'");
                assert_eq!(where_clauses[1], "chatwork.requester = '石井'");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_status_transition_commands() {
        let cli = Cli::parse_from(["taskforce", "status", "10", "waiting"]);
        match cli.command {
            Commands::Status { id, status } => {
                assert_eq!(id, 10);
                assert_eq!(status, Some(TaskStatus::Waiting));
            }
            other => panic!("unexpected command: {other:?}"),
        }

        let cli = Cli::parse_from(["taskforce", "status", "11"]);
        match cli.command {
            Commands::Status { id, status } => {
                assert_eq!(id, 11);
                assert_eq!(status, None);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_set_command_with_json_flag() {
        let cli = Cli::parse_from([
            "taskforce",
            "set",
            "7",
            "target_sites",
            "[\"a\",\"b\"]",
            "--json",
        ]);

        match cli.command {
            Commands::Set {
                id,
                key,
                value,
                json,
            } => {
                assert_eq!(id, 7);
                assert_eq!(key, "target_sites");
                assert_eq!(value, "[\"a\",\"b\"]");
                assert!(json);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_serve_command() {
        let cli = Cli::parse_from(["taskforce", "serve"]);
        assert!(matches!(cli.command, Commands::Serve));
    }

    #[test]
    fn parses_import_chatwork_command() {
        let cli = Cli::parse_from([
            "taskforce",
            "import-chatwork",
            "https://www.chatwork.com/#!rid36219958-2111786210627420160",
        ]);

        match cli.command {
            Commands::ImportChatwork { url } => {
                assert_eq!(
                    url,
                    "https://www.chatwork.com/#!rid36219958-2111786210627420160"
                );
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
