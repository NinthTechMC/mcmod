use std::io;

use clap::{Parser, ValueEnum};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::sync::SyncCommand;
use crate::util::Project;

#[derive(Debug, Parser)]
pub struct RunCommand {
    /// The side to run
    #[arg(default_value = "client")]
    pub side: Side,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Side {
    /// Run client
    Client,
    /// Run server
    Server,
}

impl RunCommand {
    pub async fn run(self, dir: &str) -> io::Result<()> {
        let sync = SyncCommand { incremental: true };
        sync.run(dir).await?;
        let project = Project::new_in(dir)?;
        match self.side {
            Side::Client => {
                project.run_gradlew(&["runClient"]).await?;
            }
            Side::Server => {
                agree_to_eula(&project).await?;
                project.run_gradlew(&["runServer"]).await?;
            }
        }

        Ok(())
    }
}

async fn agree_to_eula(project: &Project) -> io::Result<()> {
    let mut eula_path = project.forge_root();
    eula_path.push("run");
    eula_path.push("eula.txt");
    if eula_path.exists() {
        let content = fs::read_to_string(&eula_path).await?;
        for line in content.lines() {
            if line.trim() == "eula=true" {
                return Ok(());
            }
        }
    }
    println!("Automatically agreeing to EULA to run the server.");
    println!("Please read the EULA at https://account.mojang.com/documents/minecraft_eula");

    File::create(&eula_path)
        .await?
        .write_all(b"eula=true")
        .await?;

    Ok(())
}
