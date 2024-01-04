use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::util::IoResult;

#[derive(Debug, Parser)]
pub struct InitCommand {
    /// The template to use
    pub template: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    url: String,
    branch: String,
    path: String,
    name: String,
}

impl InitCommand {
    pub async fn run(self, dir: &str) -> IoResult<()> {
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
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("Could not find template '{}'", template),
            ))?;
        }

        let template_repo = template_root.join("repos.json");
        let repo = {
            let repos_str = fs::read_to_string(template_repo).await?;
            let mut repos: BTreeMap<String, Repo> =
                serde_json::from_str(&repos_str).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Failed to parse repos.json: {}", e),
                    )
                })?;
            repos.remove(&template).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Could not find template '{}' in repos", template),
                )
            })?
        };

        if Path::new(dir).exists() {
            if fs::read_dir(dir).await?.next_entry().await?.is_some() {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("Directory '{}' is not empty", dir),
                ))?;
            }
            fs::remove_dir_all(dir).await?;
        }

        if let Some(parent) = Path::new(dir).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        println!("copying '{}' to '{}'", path.to_string_lossy(), dir);
        let r = copy_dir::copy_dir(&path, dir)?;
        if !r.is_empty() {
            for e in r {
                eprintln!("  {}", e);
            }
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to copy all files",
            ))?;
        }

        let status = Command::new("git").args(["-C", dir, "init"]).status()?;
        if !status.success() {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to initialize git repository",
            ))?;
        }

        let status = Command::new("git")
            .args([
                "-C",
                dir,
                "submodule",
                "add",
                "--branch",
                &repo.branch,
                "--name",
                &repo.name,
                &repo.url,
                &repo.path,
            ])
            .status()?;
        if !status.success() {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to add submodule",
            ))?;
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

fn templates_path() -> IoResult<PathBuf> {
    let exe = std::env::current_exe()?;
    let root = exe
        .parent() // X/target/profile
        .and_then(|x| x.parent()) // X/target
        .and_then(|x| x.parent()); // X
    match root {
        Some(x) => Ok(x.join("templates")),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find templates directory",
        ))?,
    }
}

fn list_templates() -> IoResult<()> {
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
