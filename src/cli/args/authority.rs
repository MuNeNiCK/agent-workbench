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
    #[arg(long, requires = "authority_type", conflicts_with_all = ["instruction", "source"])]
    pub(crate) path: Option<String>,
    #[arg(long = "type", requires = "path", conflicts_with = "instruction")]
    pub(crate) authority_type: Option<String>,
    #[arg(long, requires = "source", conflicts_with_all = ["path", "authority_type", "scope", "summary", "precedence"])]
    pub(crate) instruction: Option<String>,
    #[arg(long, requires = "instruction", conflicts_with = "path")]
    pub(crate) source: Option<String>,
    #[arg(long, conflicts_with = "instruction")]
    pub(crate) scope: Option<String>,
    #[arg(long, conflicts_with = "instruction")]
    pub(crate) summary: Option<String>,
    #[arg(long, conflicts_with = "instruction")]
    pub(crate) precedence: Option<i64>,
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
