use std::{collections::BTreeMap, io};

use async_trait::async_trait;

use crate::{
    gradle,
    util::{IoResult, Project},
};

use super::TemplateHandler;

pub struct Ntmc1710Handler;
#[async_trait(?Send)]
impl TemplateHandler for Ntmc1710Handler {
    fn mc_version(&self) -> &'static str {
        "1.7.10"
    }

    fn mcmod_version_key(&self) -> &'static str {
        "version"
    }

    async fn run_gradlew(&self, project: &Project, args: &[&str]) -> IoResult<()> {
        gradle::run_gradlew(&project.target_root(), 8, args).await
    }

    async fn make_gradle_properties(
        &self,
        project: &Project,
    ) -> IoResult<BTreeMap<String, String>> {
        let mcmod = project.mcmod().await?;

        if !mcmod.mixins.is_empty() {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Mixins are not supported by this template",
            ))?;
        }

        let mut map = BTreeMap::new();
        map.insert("modName".to_owned(), mcmod.name.clone());
        map.insert("modId".to_owned(), mcmod.modid.clone());
        map.insert("modVersion".to_owned(), mcmod.version.clone());
        map.insert(
            "modArtifactVersion".to_owned(),
            mcmod.artifact_version.clone(),
        );
        map.insert("modGroup".to_owned(), mcmod.group.clone());
        map.insert(
            "modArchivesBaseName".to_owned(),
            mcmod.archives_base_name.clone(),
        );
        map.insert("modGroupInternal".to_owned(), mcmod.group.replace('.', "/"));
        let ats = mcmod.access_transformers.join(" ");
        map.insert("modAccessTransformer".to_owned(), ats);
        map.insert("modCoremod".to_owned(), mcmod.coremod.clone());
        if mcmod.api.is_empty() {
            map.insert("modApiPattern".to_owned(), "".to_owned());
        } else {
            let mut api_pattern = mcmod.api.replace('.', "/");
            if !api_pattern.ends_with('/') {
                api_pattern.push('/');
            }
            api_pattern.push_str("**");
            map.insert("modApiPattern".to_owned(), api_pattern);
        }

        Ok(map)
    }
}
