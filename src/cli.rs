use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "taskforce")]
#[command(about = "Local structured task workflow")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    List,
    Add { description: String },
    Edit { id: u64, description: String },
    Delete { id: u64 },
    Done { id: u64 },
    Next,
    Serve,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{Cli, Commands};

    #[test]
    fn parses_edit_command() {
        let cli = Cli::parse_from(["taskforce", "edit", "12", "Rewrite spec"]);

        match cli.command {
            Commands::Edit { id, description } => {
                assert_eq!(id, 12);
                assert_eq!(description, "Rewrite spec");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_delete_command() {
        let cli = Cli::parse_from(["taskforce", "delete", "7"]);

        match cli.command {
            Commands::Delete { id } => assert_eq!(id, 7),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_serve_command() {
        let cli = Cli::parse_from(["taskforce", "serve"]);
        assert!(matches!(cli.command, Commands::Serve));
    }
}
