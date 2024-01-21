use std::io::{self, Write};

use clap::{Parser, ValueEnum};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

use crate::sync::SyncCommand;
use crate::template::TemplateHandler;
use crate::util::{IoResult, Project, cd};

#[derive(Debug, Parser)]
pub struct RunCommand {
    /// The command to run
    ///
    /// By default, anything starts with "client" or "server" will be
    /// mapped to "runClient" and "runServer". Other commands are passed to gradle directly
    #[arg(default_value = "client")]
    pub command: String,

    /// Whether to fully sync before running
    #[arg(short, long)]
    pub sync: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Side {
    /// Run client
    Client,
    /// Run server
    Server,
}

impl RunCommand {
    pub async fn run(self, dir: &str) -> IoResult<()> {
        let sync = SyncCommand {
            incremental: !self.sync,
        };
        sync.run(dir).await?;
        let project = Project::new_in(dir)?;
        let template_handler = project.mcmod().await?.template.new_handler();
        if let Some(c) = self.command.strip_prefix("client") {
            template_handler.run_gradlew(&project, &[&format!("runClient{c}")]).await?;
            return Ok(());
        }
        if let Some(c) = self.command.strip_prefix("server") {
            agree_to_eula(template_handler.as_ref(), &project).await?;
            template_handler.run_gradlew(&project, &[&format!("runServer{c}")]).await?;
            return Ok(());
        }

        template_handler.run_gradlew(&project, &[&self.command]).await?;
        Ok(())
    }
}

async fn agree_to_eula(template_handler: &dyn TemplateHandler, project: &Project) -> IoResult<()> {
    let eula_path = cd!(template_handler.run_dir(project)?, "eula.txt");
    if eula_path.exists() {
        let content = fs::read_to_string(&eula_path).await?;
        for line in content.lines() {
            if line.trim() == "eula=true" {
                return Ok(());
            }
        }
    }

    let env = std::env::var("MCMOD_EULA_AUTO_AGREE").unwrap_or_default();
    if env == "true" || env == "1" {
        println!("Automatically agreeing to EULA to run the server (because MCMOD_EULA_AUTO_AGREE is set)");
        println!("Please read the EULA at https://account.mojang.com/documents/minecraft_eula");
    } else {
        println!("Agreeing to the EULA is required to launch the server");
        println!("Please read the EULA at https://account.mojang.com/documents/minecraft_eula");
        println!("You can set MCMOD_EULA_AUTO_AGREE=true to automatically agree to the EULA");
        print!("Do you want to agree to the EULA? (y/N) ");
        io::stdout().flush()?;
        let mut buffer = String::new();
        let stdin = io::stdin();
        stdin.read_line(&mut buffer)?;
        if buffer.trim().to_lowercase() != "y" {
            Err(io::Error::new(io::ErrorKind::Other, "EULA not agreed"))?;
        }
    }

    File::create(&eula_path)
        .await?
        .write_all(b"eula=true")
        .await?;

    Ok(())
}
