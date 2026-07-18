use std::env;

use anyhow::{Result, bail};
use clap::Parser;

mod args;
mod runtime14;
mod update;

use args::{Cli, Command};

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = cli.root.unwrap_or(env::current_dir()?);
    match cli.command {
        Command::Init => agent_workbench::init_schema14_project(&root)
            .map(|_| println!("initialized project state\nschema_version: 14")),
        Command::Update(args) => update::handle(&root, args),
        command => {
            if !agent_workbench::is_schema14_runtime(&root)? {
                match agent_workbench::update_dry_run(&root) {
                    Ok(plan) if plan.source_schema == 13 => bail!(
                        "ordinary commands require schema14; run `agent-workbench update --dry-run` and then `agent-workbench update --reset --reason <reason>`"
                    ),
                    _ => bail!(
                        "ordinary commands require exact schema14; this ledger has no supported automatic boundary (supported reset: exact schema13 profiles only); restore a supported backup or use the matching older Agent Workbench version"
                    ),
                }
            }
            runtime14::handle(&root, command)
        }
    }
}
