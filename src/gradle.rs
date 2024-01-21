//! Gradle stuff

use std::collections::BTreeMap;
use std::process::Command;
use std::{io, path::Path};

use tokio::fs;

use crate::util::{write_file, IoResult};

/// Merge properties into a gradle.properties file without destroying comments
/// and existing properties
pub async fn merge_properties(
    gradle_properties: &Path,
    mut to_merge: BTreeMap<String, String>,
) -> IoResult<()> {
    let mut new_gradle_properties = String::new();
    if gradle_properties.exists() {
        for line in fs::read_to_string(gradle_properties).await?.lines() {
            let mut parts = line.splitn(2, '=');
            if let Some(key) = parts.next() {
                let mut key = key.trim();
                if key.starts_with("# ") {
                    key = &key[2..];
                }
                if let Some(value) = to_merge.remove(key) {
                    new_gradle_properties.push_str(&format!("{key} = {value}\n"));
                    continue;
                }
            }
            new_gradle_properties.push_str(&format!("{line}\n"));
        }
    }
    for (k, v) in to_merge {
        new_gradle_properties.push_str(&format!("{k}={v}\n"));
    }
    write_file!(gradle_properties, new_gradle_properties).await?;
    Ok(())
}

pub async fn run_gradlew(dir: &Path, java_version: u32, args: &[&str]) -> IoResult<()> {
    let jdk_home = format!("JDK{java_version}_HOME");
    let jdk_home = match std::env::var(&jdk_home) {
        Ok(x) => x,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Could not find {jdk_home} environment variable"),
        ))?,
    };
    let java_home = Path::new(&jdk_home);
    let gradlew = if cfg!(windows) {
        dir.join("gradlew.bat")
    } else {
        dir.join("gradlew")
    };

    let status = Command::new(gradlew)
        .args(args)
        .current_dir(dir)
        .env("JAVA_HOME", java_home)
        .status()?;
    if !status.success() {
        Err(io::Error::new(io::ErrorKind::Other, "gradlew failed"))?;
    }
    Ok(())
}
