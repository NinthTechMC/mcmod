use std::cell::OnceCell;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs::{self, File};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug)]
pub struct Project {
    /// Root directory of the project
    pub root: PathBuf,
    /// The mcmod.json file
    mcmod: OnceCell<Mcmod>,
    /// The Java version, maybe 8 or greater
    java_version: OnceCell<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mcmod {
    modid: String,
    pub template: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub update_url: String,
    pub author_list: Vec<String>,
    pub credits: String,
    pub logo_file: String,
    pub screenshots: Vec<String>,
    pub dependencies: Vec<String>,
    pub libs: Vec<String>,

    #[serde(default = "default_prefix")]
    pub prefix: String,
}

impl Mcmod {
    pub fn original_modid(&self) -> &str {
        &self.modid
    }
    pub fn modid(&self) -> String {
        self.modid.to_lowercase()
    }
}

fn default_prefix() -> String {
    "pistonmc".to_string()
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
        Ok(Self {
            root: cur_path.to_path_buf(),
            mcmod: OnceCell::new(),
            java_version: OnceCell::new(),
        })
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

    pub async fn source_root(&self) -> io::Result<PathBuf> {
        let mut p = self.root.join("src");
        let mcmod = self.mcmod_json().await?;
        for part in mcmod.prefix.split('.') {
            p.push(part);
        }
        Ok(p)
    }

    pub fn forge_root(&self) -> PathBuf {
        self.root.join("forge")
    }

    pub fn assets_root(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub async fn group(&self) -> io::Result<String> {
        let mcmod = self.mcmod_json().await?;
        let prefix = &mcmod.prefix;
        let modid = mcmod.modid();
        if prefix.is_empty() {
            return Ok(modid);
        }
        Ok(format!( "{prefix}.{modid}"))
    }

    pub async fn java_version(&self) -> io::Result<u32> {
        if let Some(x) = self.java_version.get() {
            return Ok(*x);
        }
        let file = File::open(self.forge_root().join("build.gradle")).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut version = None;
        while let Some(line) = lines.next_line().await? {
            if let Some(line) = line.strip_prefix("sourceCompatibility =") {
                let line = line.trim();
                if line == "1.8" {
                    version = Some(8u32);
                } else {
                    let v = line.parse::<u32>().map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "Invalid Java version")
                    })?;
                    version = Some(v);
                }
                break;
            }
        }
        match version {
            None => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Could not find Java version",
            )),
            Some(v) => Ok(*self.java_version.get_or_init(|| v)),
        }
    }

    pub async fn run_gradlew(&self, args: &[&str]) -> io::Result<()> {
        let java_version = self.java_version().await?;
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
