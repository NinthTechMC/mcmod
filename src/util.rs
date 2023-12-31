use std::borrow::Cow;
use std::cell::OnceCell;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct Project {
    /// Root directory of the project
    pub root: PathBuf,
    /// The mcmod.json file
    mcmod: OnceCell<Mcmod>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mcmod {
    /// Version of Java to use
    pub java: u32,
    /// Version of template. Currently unused
    pub template: String,
    /// Mod Version. Can be any string.
    pub version: String,
    /// Override default gradle settings
    pub gradle_override: Option<GradleOverride>,
    /// The coremod class
    pub coremod: Option<String>,
    /// Libraries to download
    pub libs: Vec<String>,

    // mcmod.info fields
    pub modid: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub update_url: String,
    pub author_list: Vec<String>,
    pub credits: String,
    pub logo_file: String,
    pub screenshots: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradleOverride {
    pub enabled: bool,
    pub version: String,
    pub group: String,
    pub archive_base_name: String,
}

impl Mcmod {
    pub fn gradle_version(&self) -> &str {
        match self.gradle_override.as_ref() {
            Some(x) if x.enabled => &x.version,
            _ => &self.version,
        }
    }

    pub fn gradle_group(&self) -> Cow<'_, str> {
        match self.gradle_override.as_ref() {
            Some(x) if x.enabled => Cow::Borrowed(&x.group),
            _ => format!("pistonmc.{}", self.modid.to_lowercase()).into(),
        }
    }

    pub fn archive_base_name(&self) -> Cow<'_, str> {
        match self.gradle_override.as_ref() {
            Some(x) if x.enabled => Cow::Borrowed(&x.archive_base_name),
            _ => self.name.to_lowercase().replace(' ', "-").into(),
        }
    }
}

impl Project {
    /// Initialize a new project context in the given directory
    pub fn new_in(dir: &str) -> io::Result<Self> {
        let path = dunce::canonicalize(Path::new(dir))?;
        let mut cur_path = path.as_ref();
        while !path.join("mcmod.json").exists() {
            if let Some(parent) = path.parent() {
                cur_path = parent;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not find project root",
                ));
            }
        }
        Ok(Self::new_root(cur_path.to_path_buf()))
    }

    pub fn new_root(root: PathBuf) -> Self {
        Self {
            root,
            mcmod: OnceCell::new(),
        }
    }

    /// Get the mcmod.json data
    pub async fn mcmod_json(&self) -> io::Result<&Mcmod> {
        if let Some(x) = self.mcmod.get() {
            return Ok(x);
        }
        let mcmod_path = self.root.join("mcmod.json");
        let mcmod = fs::read_to_string(mcmod_path).await?;
        let mcmod: Mcmod = match serde_json::from_str(&mcmod) {
            Ok(mcmod) => mcmod,
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };
        Ok(self.mcmod.get_or_init(|| mcmod))
    }

    pub async fn write_mcmod_info(&self) -> io::Result<()> {
        let mcmod = self.mcmod_json().await?;
        let info = json!([{
            "modid": mcmod.modid,
            "name": mcmod.name,
            "description": mcmod.description,
            "version": "${version}",
            "mcversion": "${mcversion}",
            "url": mcmod.url,
            "updateUrl": mcmod.update_url,
            "authorList": mcmod.author_list,
            "credits": mcmod.credits,
            "logoFile": mcmod.logo_file,
            "screenshots": mcmod.screenshots,
            "dependencies": mcmod.dependencies,
        }]);
        let info_str = serde_json::to_string_pretty(&info)?;
        let mut info_path = self.forge_root();
        info_path.push("src");
        info_path.push("main");
        info_path.push("resources");
        info_path.push("mcmod.info");
        File::create(info_path)
            .await?
            .write_all(info_str.as_bytes())
            .await?;
        Ok(())
    }

    pub async fn write_pack_mcmeta(&self) -> io::Result<()> {
        let mcmod = self.mcmod_json().await?;
        let pack = json!({
            "pack": {
                "pack_format": 1,
                "description": format!("Resources used for {}", mcmod.name),
            }
        });
        let pack_str = serde_json::to_string_pretty(&pack)?;
        let mut pack_path = self.forge_root();
        pack_path.push("src");
        pack_path.push("main");
        pack_path.push("resources");
        pack_path.push("pack.mcmeta");
        File::create(pack_path)
            .await?
            .write_all(pack_str.as_bytes())
            .await?;
        Ok(())
    }

    pub fn source_root(&self) -> PathBuf {
        self.root.join("src")
    }

    pub async fn source_group(&self) -> io::Result<String> {
        let mut current = self.source_root();
        let mut source_group = String::new();
        // if current contains a single entry and is dir, continue go into it
        while current.is_dir() {
            let mut dir = fs::read_dir(&current).await?;
            let entry = match dir.next_entry().await? {
                None => break,
                Some(x) => x,
            };
            if dir.next_entry().await?.is_some() {
                break;
            }
            if entry.file_type().await?.is_dir() {
                if !source_group.is_empty() {
                    source_group.push('.');
                }
                let file_name = entry.file_name();
                let name = match file_name.to_str() {
                    Some(x) => x,
                    None => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid source group name",
                        ))
                    }
                };
                source_group.push_str(name);
            } else {
                break;
            }
            current = entry.path();
        }

        Ok(source_group)
    }

    pub fn forge_root(&self) -> PathBuf {
        self.root.join("forge")
    }

    pub fn assets_root(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub async fn run_gradlew(&self, args: &[&str]) -> io::Result<()> {
        let java_version = self.mcmod_json().await?.java;
        let jdk_home = format!("JDK{java_version}_HOME");
        let jdk_home = match std::env::var(&jdk_home) {
            Ok(x) => x,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Could not find {jdk_home} environment variable"),
                ))
            }
        };
        let java_home = Path::new(&jdk_home);
        let gradlew = if cfg!(windows) {
            self.forge_root().join("gradlew.bat")
        } else {
            self.forge_root().join("gradlew")
        };

        let status = Command::new(gradlew)
            .args(args)
            .current_dir(&self.forge_root())
            .env("JAVA_HOME", java_home)
            .status()?;
        if !status.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "gradlew failed"));
        }
        Ok(())
    }
}
