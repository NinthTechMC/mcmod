use std::io;

use clap::{Parser, Subcommand};

mod build;
mod init;
mod run;
mod sync;
mod util;

use init::InitCommand;
use run::RunCommand;
use sync::SyncCommand;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = cli.run().await {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

/// MC modding tool
#[derive(Debug, Parser)]
pub struct Cli {
    /// Directory to run the command in
    #[arg(short = 'C', long, default_value = ".")]
    pub dir: String,

    /// Command to run
    #[clap(subcommand)]
    pub command: CliCommand,
}

impl Cli {
    pub async fn run(self) -> io::Result<()> {
        match self.command {
            CliCommand::Sync(sync) => sync.run(&self.dir).await,
            CliCommand::Init(init) => init.run(&self.dir).await,
            CliCommand::Build => crate::build::run_build(&self.dir).await,
            CliCommand::Run(run) => run.run(&self.dir).await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Syncs the project state
    Sync(SyncCommand),
    /// Build the project
    Build,
    /// Run the project
    Run(RunCommand),
    /// Initialize a new project in the current directory
    Init(InitCommand),
}
