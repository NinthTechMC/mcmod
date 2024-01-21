//! The mcmod.yaml front end properties

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_recursion::async_recursion;
use ninja_writer::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::task::JoinSet;
use tokio::{fs, io};

use crate::template::Template;
use crate::util::{join_join_set, IoResult, Project};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Mcmod {
    /// Template being used
    pub template: Template,
    /// Name of the mod
    pub name: String,
    /// Mod id
    pub modid: String,
    /// Mod description
    pub description: String,
    /// Url of the mod
    #[serde(default)]
    pub url: String,
    /// Update url
    #[serde(default)]
    pub update_url: String,
    /// List of authors
    #[serde(default)]
    pub authors: Vec<String>,
    /// Credit info
    #[serde(default)]
    pub credits: String,
    /// Logo file
    #[serde(default)]
    pub logo: String,
    /// Screenshot files
    #[serde(default)]
    pub screenshots: Vec<String>,
    /// Mod Version. Can be any string.
    pub version: String,
    /// Version to use for artifacts
    #[serde(default)]
    pub artifact_version: String,
    /// The group
    #[serde(default)]
    pub group: String,
    /// The archive base name
    #[serde(default)]
    pub archives_base_name: String,
    /// The api package
    #[serde(default)]
    pub api: String,
    /// The coremod class
    #[serde(default)]
    pub coremod: String,
    /// The access transformer file
    #[serde(default)]
    pub access_transformers: Vec<String>,
    /// The mixin package
    #[serde(default)]
    pub mixins: String,
    /// Libraries to download
    #[serde(default)]
    pub libs: Vec<String>,
    /// Mods to download
    #[serde(default)]
    pub mods: Vec<String>,
    /// Gradle properties overrides
    #[serde(default)]
    pub gradle_overrides: BTreeMap<String, String>,
    /// Paths to copy to the template
    #[serde(default)]
    pub copy_paths: Vec<CopySpec>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CopySpec {
    Simple(String),
    SourceTarget(String, String),
}

impl Mcmod {
    /// Apply defaults to missing fields
    pub async fn apply_defaults(&mut self, project: &Project) -> IoResult<()> {
        if self.update_url.is_empty() && !self.url.is_empty() {
            self.update_url = self.url.clone();
        }
        if self.artifact_version.is_empty() {
            self.artifact_version = self.version.clone();
        }
        if self.group.is_empty() {
            self.group = project.source_group().await?;
        }
        if self.archives_base_name.is_empty() {
            self.archives_base_name = self.name.replace(' ', "-");
        }

        Ok(())
    }

    /// Create the content of the mcmod.info file
    pub fn create_mcmod_info(&self) -> IoResult<String> {
        let info = json!([{
            "modid": self.modid,
            "name": self.name,
            "description": self.description,
            "version": "${version}",
            "mcversion": "${mcversion}",
            "url": self.url,
            "updateUrl": self.update_url,
            "authorList": self.authors,
            "credits": self.credits,
            "logoFile": self.logo,
            "screenshots": self.screenshots,
            "dependencies": [],
        }]);
        match serde_json::to_string_pretty(&info) {
            Ok(x) => Ok(x),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e))?,
        }
    }

    /// Create the content of the pack.mcmeta file
    pub fn create_pack_mcmeta(&self) -> IoResult<String> {
        let pack = json!({
            "pack": {
                "pack_format": 1,
                "description": format!("Resources used for {}", self.name),
            }
        });
        match serde_json::to_string_pretty(&pack) {
            Ok(x) => Ok(x),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e))?,
        }
    }

    /// Create the content of build.ninja
    pub async fn create_build_ninja(&self, root: &Path, target_root: &Path) -> IoResult<String> {
        let ninja = Ninja::new();
        ninja.comment("Incremental build file for copying source and assets");
        ninja.comment("Please run `mcmod sync` to update this file when mcmod.yaml, or when the file structure changes");

        let cp = if cfg!(windows) {
            Rule::new("cp", "coreutils cp $in $out")
        } else {
            Rule::new("cp", "cp $in $out")
        };
        let cp = cp.description("Copying $in").add_to(&ninja);

        let mut join_set = JoinSet::new();
        for copy_path in &self.copy_paths {
            if let CopySpec::SourceTarget(s, t) = copy_path {
                if s == "null" {
                    let target = target_root.join(t);
                    if target.exists() {
                        if target.is_dir() {
                            fs::remove_dir_all(&target).await?;
                        } else {
                            fs::remove_file(&target).await?;
                        }
                    }
                }
            }
        }

        for copy_path in &self.copy_paths {
            let (source, target) = match copy_path {
                CopySpec::Simple(s) => (s, s),
                CopySpec::SourceTarget(s, t) => (s, t),
            };
            if source == "null" {
                continue;
            }
            let source = root.join(source);
            if !source.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "Source path '{}' does not exist. Please remove it from mcmod.yaml",
                        source.display()
                    ),
                ))?;
            }
            let source = Arc::new(source);
            let target = Arc::new(target_root.join(target));
            let cp = cp.clone();
            join_set.spawn(async move { add_copy_edge(source, target, cp, PathBuf::new()).await });
        }
        join_join_set!(join_set).await?;

        Ok(ninja.to_string())
    }
}

#[async_recursion]
async fn add_copy_edge(
    source_root: Arc<PathBuf>,
    target_root: Arc<PathBuf>,
    cp: RuleRef,
    path: PathBuf,
) -> IoResult<()> {
    let source_path = source_root.join(&path);
    let target_path = target_root.join(&path);
    if source_path.is_dir() {
        if !target_path.exists() {
            fs::create_dir_all(&target_path).await?;
        }
        let mut join_set = JoinSet::new();
        let mut dir = fs::read_dir(source_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = path.join(entry.file_name());
            let source_root = Arc::clone(&source_root);
            let target_root = Arc::clone(&target_root);
            let cp = cp.clone();
            join_set.spawn(async move { add_copy_edge(source_root, target_root, cp, path).await });
        }
        join_join_set!(join_set).await?;
    } else {
        cp.build([escape_build(&target_path.display().to_string())])
            .with([escape_build(&source_path.display().to_string())]);
    }

    Ok(())
}
