use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use tokio::fs;

#[derive(Debug, Parser)]
pub struct InitCommand {
    /// The template to use
    pub template: Option<String>,
}

impl InitCommand {
    pub async fn run(self, dir: &str) -> io::Result<()> {
        let template = match self.template {
            Some(t) => t,
            None => {
                list_templates()?;
                return Ok(());
            }
        };
        let template_root = templates_path()?;
        let path = template_root.join(&template);

        if !path.exists() {
            list_templates()?;
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Could not find template '{}'", template),
            ));
        }

        if Path::new(dir).exists() {
            if fs::read_dir(dir).await?.next_entry().await?.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("Directory '{}' is not empty", dir),
                ));
            }
        }
        fs::remove_dir_all(dir).await?;

        println!("copying '{}' to '{}'", path.to_string_lossy(), dir);
        let r = copy_dir::copy_dir(&path, dir)?;
        if !r.is_empty() {
            for e in r {
                eprintln!("  {}", e);
            }
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to copy all files",
            ));
        }

        let status = Command::new("git").args(["-C", dir, "init"]).status()?;
        if !status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to initialize git repository",
            ));
        }

        println!();
        println!("done!");
        println!("next steps:");
        println!("  1. cd {dir}");
        println!("  2. edit mcmod.json");
        println!("  3. mcmod sync");

        Ok(())
    }
}

fn templates_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?; // X/target/profile/mcmod
    let root = exe
        .parent() // X/target/profile
        .and_then(|x| x.parent()) // X/target
        .and_then(|x| x.parent()); // X
    match root {
        Some(x) => Ok(x.join("templates")),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find templates directory",
        )),
    }
}

fn list_templates() -> io::Result<()> {
    let path = templates_path()?;
    let mut templates = Vec::new();
    for entry in path.read_dir()? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            templates.push(entry.file_name());
        }
    }
    println!("available templates:");
    for template in templates {
        println!("  {}", template.to_string_lossy());
    }
    Ok(())
}
