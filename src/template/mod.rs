use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{fs, io};

use crate::util::{self, cd, IoResult, Project};

mod gtnh;
mod ntmc;

#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateDef {
    pub url: String,
    pub branch: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Template {
    #[serde(rename = "ntmc-1.7.10")]
    Ntmc1710,
    #[serde(rename = "gtnh-1.7.10")]
    Gtnh1710,
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_string(self).unwrap();
        write!(f, "{}", s.trim_matches('"'))
    }
}

impl Template {
    pub fn new_handler(&self) -> Box<dyn TemplateHandler> {
        match self {
            Self::Ntmc1710 => Box::new(ntmc::Ntmc1710Handler),
            Self::Gtnh1710 => Box::new(gtnh::Gtnh1710Handler),
        }
    }
}

#[async_trait(?Send)]
pub trait TemplateHandler {
    /// Called to setup the template after cloning.
    ///
    /// Templates usually run "setupDecompWorkspace" here, but there can be extra setup steps.
    async fn setup_project(&self, project: &Project) -> IoResult<()> {
        self.run_gradlew(project, &["setupDecompWorkspace"]).await?;
        Ok(())
    }
    /// Called to setup eclipse workspace
    async fn setup_eclipse(&self, project: &Project) -> IoResult<()> {
        self.run_gradlew(project, &["eclipse"]).await?;
        Ok(())
    }
    /// Called to build
    async fn build(&self, project: &Project) -> IoResult<()> {
        self.run_gradlew(project, &["build"]).await?;
        Ok(())
    }
    /// Run gradlew with args. Should set java version and call gradle::run_gradlew
    async fn run_gradlew(&self, project: &Project, args: &[&str]) -> IoResult<()>;
    /// The build output dir
    fn output_dir(&self, project: &Project) -> IoResult<PathBuf> {
        Ok(cd!(project.target_root(), "build", "libs"))
    }
    /// The dependency libs dir
    fn libs_dir(&self, project: &Project) -> IoResult<PathBuf> {
        Ok(cd!(project.target_root(), "libs"))
    }
    /// The runtime minecraft dir
    fn run_dir(&self, project: &Project) -> IoResult<PathBuf> {
        Ok(cd!(project.target_root(), "run"))
    }
    /// Make a map of gradle properties to combine with gradle.properties in the template
    async fn make_gradle_properties(&self, project: &Project)
        -> IoResult<BTreeMap<String, String>>;
}

pub async fn read_templates() -> IoResult<BTreeMap<String, TemplateDef>> {
    let templates_json_path = templates_path()?;
    let templates_json = fs::read_to_string(templates_json_path).await?;
    let templates: BTreeMap<String, TemplateDef> =
        serde_json::from_str(&templates_json).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse templates.json: {}", e),
            )
        })?;
    Ok(templates)
}

pub fn templates_path() -> IoResult<PathBuf> {
    Ok(cd!(util::tool_root()?, "templates.json"))
}

pub fn list_templates(templates: &BTreeMap<String, TemplateDef>) {
    println!("available templates:");
    for template in templates.keys() {
        println!("  {template}");
    }
}
