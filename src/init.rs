use std::io;
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use tokio::fs;

use crate::template;
use crate::util::{mkdir, IoResult, confirm_yn, tool_root, cd, write_file};

#[derive(Debug, Parser)]
pub struct InitCommand {
    /// The template to use
    pub template: Option<String>,
}

impl InitCommand {
    pub async fn run(self, dir: &str) -> IoResult<()> {
        let dir_str = dir;
        let dir = PathBuf::from(dir);
        if dir.exists() {
            if fs::read_dir(&dir).await?.next_entry().await?.is_some() {
                println!("Directory '{}' is not empty!", dir_str);
                println!("You will be prompted for each file that would be overwritten.");
                println!("Continue?");
                if !confirm_yn()? {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Operation cancelled",
                    ))?;
                }
            }
        } else {
            mkdir!(&dir).await?;
        }

        if !dir.join(".git").exists() {
            let status = Command::new("git").args(["-C", dir_str, "init"]).status()?;
            if !status.success() {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to initialize git repository",
                ))?;
            }
        }

        let mut templates = template::read_templates().await?;

        let template = match self.template {
            Some(t) => t,
            None => {
                println!("Please specify a template!");
                template::list_templates(&templates);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "No template specified",
                ))?;
            }
        };

        templates.remove(&template).ok_or_else(|| {
            println!("Unknown template '{template}'");
            io::Error::new(
                io::ErrorKind::Other,
                "Unknown template",
            )
        })?;

        let init_dir = cd!(tool_root()?, "init");
        let mut init_dir_iter = fs::read_dir(&init_dir).await?;
        while let Some(entry) = init_dir_iter.next_entry().await? {
            let target_path = dir.join(entry.file_name());
            if target_path.exists() {
                println!("overwrite '{}'?", target_path.display());
                if !confirm_yn()? {
                    continue;
                }
                if target_path.is_dir() {
                    fs::remove_dir_all(&target_path).await?;
                }
            }
            let source_dir = entry.path();
            println!("copying '{}' to '{}'", entry.file_name().to_string_lossy(), target_path.display());
            if source_dir.is_dir() {
                let r = copy_dir::copy_dir(&source_dir, &target_path)?;
                if !r.is_empty() {
                    for e in r {
                        eprintln!("  {}", e);
                    }
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Failed to copy all files",
                    ))?;
                }
            } else {
                fs::copy(&source_dir, &target_path).await?;
            }
        }

        let mcmod_path = dir.join("mcmod.yaml");
        let mcmod = fs::read_to_string(&mcmod_path).await?;
        let mcmod = mcmod.replace("INIT_TEMPLATE", &template);
        write_file!(&mcmod_path, mcmod).await?;

        println!();
        println!("done!");
        println!("next steps:");
        println!("  1. cd {dir_str}");
        println!("  2. edit mcmod.yaml");
        println!("  3. mcmod sync");

        Ok(())
    }
}

