use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityCommand {
    Add(AuthorityAddArgs),
    Event {
        #[command(subcommand)]
        command: AuthorityEventCommand,
    },
    List(AuthorityListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityAddArgs {
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long = "type")]
    pub(crate) authority_type: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) summary: Option<String>,
    #[arg(long, default_value_t = 90)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityEventCommand {
    Add(AuthorityEventAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityEventAddArgs {
    #[arg(long = "type")]
    pub(crate) event_type: String,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) source: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityListArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
}
