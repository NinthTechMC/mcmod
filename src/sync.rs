use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use quick_xml::events::{BytesStart, BytesText, Event};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use async_recursion::async_recursion;
use clap::Parser;
use ninja_writer::*;
use quick_xml::{Reader, Writer};
use reqwest::Client;

use crate::util::Project;

#[derive(Debug, Parser)]
pub struct SyncCommand {
    /// If syncing incrementally.
    ///
    /// If true, the directory structure and mcmod.json is assumed to be the same.
    /// Only updated source and asset files are synced.
    #[arg(short, long)]
    pub incremental: bool,
}

impl SyncCommand {
    pub async fn run(mut self, dir: &str) -> io::Result<()> {
        let project = Project::new_in(dir)?;
        let decomp_marker = project.forge_root().join(".decomp-workspace");
        if !decomp_marker.exists() && !self.incremental {
            println!("forcing non-incremental sync since decomp workspace is missing");
            self.incremental = false;
        }

        if !self.incremental {
            update_gradle(&project).await?;
        }
        sync_source(&project, self.incremental).await?;

        if !self.incremental {
            project.write_mcmod_info().await?;
            project.write_pack_mcmeta().await?;
            sync_libs(&project).await?;
            if !decomp_marker.exists() {
                project.run_gradlew(&["setupDecompWorkspace"]).await?;
                File::create(&decomp_marker).await?;
            }
            update_eclipse(&project).await?;
        }

        Ok(())
    }
}

async fn sync_source(project: &Project, incremental: bool) -> io::Result<()> {
    println!("syncing source");
    let build_ninja = project.root.join("build.ninja");
    if !build_ninja.exists() || !incremental {
        let mut forge_source_root = project.forge_root();
        forge_source_root.push("src");
        if forge_source_root.exists() {
            fs::remove_dir_all(&forge_source_root).await?;
        }
        write_build_ninja(&build_ninja, &project).await?;
    }

    let result = Command::new("ninja").current_dir(&project.root).status()?;

    if !result.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "ninja failed"));
    }
    Ok(())
}

async fn update_gradle(project: &Project) -> io::Result<()> {
    let mcmod = project.mcmod_json().await?;

    let gradle_generated_root = {
        let mut p = project.forge_root();
        p.push("gradle");
        p.push("generated");
        p
    };
    fs::create_dir_all(&gradle_generated_root).await?;

    let mut join_set = JoinSet::new();
    join_set.spawn(write_compatibility_gradle(
        gradle_generated_root.clone(),
        mcmod.java,
    ));
    join_set.spawn(write_coremod_gradle(
        gradle_generated_root.clone(),
        mcmod.coremod.as_ref().cloned(),
    ));
    let group_gradle_future = write_group_gradle(
        gradle_generated_root,
        mcmod.gradle_version().to_owned(),
        mcmod.gradle_group().into_owned(),
        mcmod.archive_base_name().into_owned(),
        mcmod.modid.to_owned(),
        mcmod.version.to_owned(),
        project.source_group().await?,
    );
    join_set.spawn(group_gradle_future);

    while let Some(result) = join_set.join_next().await {
        result??;
    }

    Ok(())
}

async fn write_compatibility_gradle(gradle_generated_root: PathBuf, java: u32) -> io::Result<()> {
    let compability_str = {
        let version = if java == 8 {
            "1.8".to_string()
        } else {
            java.to_string()
        };
        format!(
            r###"
sourceCompatibility = {version}
targetCompatibility = {version}
"###
        )
    };

    File::create(gradle_generated_root.join("compatibility.gradle"))
        .await?
        .write_all(compability_str.as_bytes())
        .await?;

    Ok(())
}

async fn write_coremod_gradle(
    gradle_generated_root: PathBuf,
    coremod: Option<String>,
) -> io::Result<()> {
    let coremod_str = match coremod {
        Some(class) => {
            format!(
                r###"jar {{
    manifest {{
       attributes 'FMLCorePlugin': '{class}'
       attributes 'FMLCorePluginContainsFMLMod': 'true'
    }}
}}
"###
            )
        }
        None => "".to_owned(),
    };

    File::create(gradle_generated_root.join("coremod.gradle"))
        .await?
        .write_all(coremod_str.as_bytes())
        .await?;

    Ok(())
}

async fn write_group_gradle(
    gradle_generated_root: PathBuf,
    version: String,
    group: String,
    archive_base_name: String,
    code_modid: String,
    code_version: String,
    code_group: String,
) -> io::Result<()> {
    let code_group_internal = code_group.replace('.', "/");

    let group_str = format!(
        r###"version = '{version}'
group = '{group}'
archivesBaseName = '{archive_base_name}'

minecraft {{
    replaceIn "ModInfo.java"
    replaceIn "CoremodInfo.java"

    replace "@modid@", "{code_modid}"
    replace "@version@", "{code_version}"
    replace "@group@", "{code_group}"
    replace "@groupInternal@", "{code_group_internal}"

}}
"###
    );

    File::create(gradle_generated_root.join("group.gradle"))
        .await?
        .write_all(group_str.as_bytes())
        .await?;

    Ok(())
}

async fn write_build_ninja(file: &Path, project: &Project) -> io::Result<()> {
    let ninja = Ninja::new();
    ninja.comment("Incremental build file for copying source and assets");
    ninja.comment("Please run `mcmod sync` to update this file when file structure changes");

    let cp = ninja_copy_rule().description("Copying $in").add_to(&ninja);

    let source_root = project.source_root();

    let mut target_root = project.forge_root();
    target_root.push("src");
    target_root.push("main");
    target_root.push("java");

    add_copy_edge(
        Arc::new(source_root),
        Arc::new(target_root),
        cp.clone(),
        PathBuf::new(),
    )
    .await?;

    let assets_root = project.assets_root();

    let mut target_root = project.forge_root();
    target_root.push("src");
    target_root.push("main");
    target_root.push("resources");
    target_root.push("assets");
    add_copy_edge(
        Arc::new(assets_root),
        Arc::new(target_root),
        cp,
        PathBuf::new(),
    )
    .await?;

    File::create(file)
        .await?
        .write_all(ninja.to_string().as_bytes())
        .await?;
    Ok(())
}

#[async_recursion]
async fn add_copy_edge(
    source_root: Arc<PathBuf>,
    target_root: Arc<PathBuf>,
    cp: RuleRef,
    path: PathBuf,
) -> io::Result<()> {
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
        while let Some(result) = join_set.join_next().await {
            result??;
        }
    } else {
        cp.build([escape_build(&target_path.display().to_string())])
            .with([escape_build(&source_path.display().to_string())]);
    }

    Ok(())
}

#[cfg(windows)]
fn ninja_copy_rule() -> Rule {
    Rule::new("cp", "coreutils cp $in $out")
}

#[cfg(not(windows))]
fn ninja_copy_rule() -> Rule {
    Rule::new("cp", "cp $in $out")
}

async fn sync_libs(project: &Project) -> io::Result<()> {
    let libs_root = project.forge_root().join("libs");
    if !libs_root.exists() {
        fs::create_dir_all(&libs_root).await?;
    }
    let libs = &project.mcmod_json().await?.libs;
    let mut needs_download = libs.iter().map(|lib| lib.as_str()).collect::<Vec<_>>();
    let mut dir = fs::read_dir(&libs_root).await?;
    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        let name = match file_name.to_str() {
            Some(name) => name,
            None => continue,
        };
        match needs_download.iter().position(|lib| lib == &name) {
            Some(i) => {
                // up to date
                needs_download.swap_remove(i);
            }
            None => {
                let path = entry.path();
                println!("removing '{}'", path.display());
                fs::remove_file(path).await?;
            }
        }
    }
    let mut join_set = JoinSet::new();
    let (send, mut recv) = mpsc::channel::<io::Result<String>>(100);
    let client = Arc::new(Client::new());
    join_set.spawn(async move {
        let mut error = None;
        while let Some(result) = recv.recv().await {
            if error.is_some() {
                continue;
            }
            match result {
                Ok(url) => {
                    println!("downloaded '{}'", url);
                }
                Err(e) => {
                    error = Some(e);
                    recv.close();
                }
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    });
    for lib in needs_download {
        let (url, path) = if lib.starts_with("http") {
            let url = lib.to_owned();
            let file_name = match Path::new(&url).file_name() {
                Some(name) => name,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Cannot find file name in url '{url}'"),
                    ));
                }
            };
            let path = libs_root.join(file_name);
            (url, path)
        } else {
            let url = format!("https://cdn.pistonite.org/minecraft/devjars/{lib}");
            let path = libs_root.join(lib);
            (url, path)
        };
        println!("downloading '{url}'");
        let client = Arc::clone(&client);
        let send = send.clone();
        join_set.spawn(async move {
            let result = download_devjar(client, &url, &path).await.map(|_| url);
            let _ = send.send(result).await;
            Ok(())
        });
    }
    drop(send);
    while let Some(result) = join_set.join_next().await {
        result??;
    }
    Ok(())
}

async fn download_devjar(client: Arc<Client>, url: &str, path: &Path) -> io::Result<()> {
    let bytes_result = async { client.get(url).send().await?.bytes().await }.await;

    let bytes = match bytes_result {
        Ok(response) => response,
        Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
    };

    File::create(path).await?.write_all(&bytes).await?;

    Ok(())
}

async fn update_eclipse(project: &Project) -> io::Result<()> {
    project.run_gradlew(&["eclipse"]).await?;
    let output_file = project.root.join(".classpath");
    let writer = std::io::BufWriter::new(std::fs::File::create(&output_file)?);
    let classpath_file = project.forge_root().join(".classpath");
    let input = fs::read_to_string(&classpath_file)
        .await?
        .replace("\r\n", "\n");
    let result = async {
        let mut reader = Reader::from_str(&input);
        let mut writer = Writer::new_with_indent(writer, b' ', 4);
        let mut buf = Vec::new();

        loop {
            let event = reader.read_event_into(&mut buf)?;
            match event {
                Event::Start(e) => {
                    if e.name().as_ref() == b"classpathentry" {
                        // collect attributes
                        let mut attributes = Vec::new();
                        let mut path = None;
                        for attr in e.attributes() {
                            let attr = attr?;
                            if attr.key.as_ref() == b"path" {
                                path = Some(attributes.len());
                            }
                            attributes.push(attr);
                        }
                        if let Some(i) = path {
                            let attr = attributes.get_mut(i).unwrap();
                            match attr.value.as_ref() {
                                b"src/main/java" => {
                                    attr.value = Cow::Borrowed(b"src");
                                }
                                b"src/main/resources" => {
                                    attr.value = Cow::Borrowed(b"assets");
                                    let attr = attributes
                                        .iter_mut()
                                        .find(|k| k.key.as_ref() == b"output")
                                        .unwrap();
                                    attr.value = Cow::Borrowed(b"bin/assets");
                                }
                                _ => {}
                            }
                        }
                        let mut e = BytesStart::new("classpathentry");
                        e.extend_attributes(attributes);
                        writer.write_event(Event::Start(e))?;
                    } else {
                        writer.write_event(Event::Start(e))?;
                    }
                }
                Event::Eof => break,
                e => writer.write_event(e)?,
            }
        }

        Ok::<(), quick_xml::Error>(())
    }
    .await;

    if let Err(e) = result {
        return Err(io::Error::new(io::ErrorKind::InvalidData, e));
    }

    fs::remove_file(classpath_file).await?;

    let output_file = project.root.join(".project");
    let project_name = match project.root.file_name().and_then(|s| s.to_str()) {
        Some(name) => name,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Cannot determine project name from root path",
            ));
        }
    };
    let writer = std::io::BufWriter::new(std::fs::File::create(&output_file)?);
    let project_file = project.forge_root().join(".project");
    let input = fs::read_to_string(&project_file)
        .await?
        .replace("\r\n", "\n");
    let result = async {
        let mut reader = Reader::from_str(&input);
        let mut writer = Writer::new_with_indent(writer, b' ', 4);
        let mut buf = Vec::new();

        let mut level = 0;
        let mut found_name = false;

        loop {
            let event = reader.read_event_into(&mut buf)?;
            match event {
                Event::Start(e) => {
                    if !found_name {
                        if level == 1 && e.name().as_ref() == b"name" {
                            found_name = true;
                        }
                    }
                    level += 1;
                    writer.write_event(Event::Start(e))?;
                }
                Event::End(e) => {
                    level -= 1;
                    writer.write_event(Event::End(e))?;
                }
                Event::Text(e) => {
                    if found_name {
                        writer.write_event(Event::Text(BytesText::new(&project_name)))?;
                        found_name = false;
                    } else {
                        writer.write_event(Event::Text(e))?;
                    }
                }
                Event::Eof => break,
                e => writer.write_event(e)?,
            }
        }

        Ok::<(), quick_xml::Error>(())
    }
    .await;

    fs::remove_file(project_file).await?;

    if let Err(e) = result {
        return Err(io::Error::new(io::ErrorKind::InvalidData, e));
    }

    Ok(())
}
