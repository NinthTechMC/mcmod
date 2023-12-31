use std::borrow::Cow;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use quick_xml::events::{BytesStart, BytesText, Event};
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use async_recursion::async_recursion;
use clap::Parser;
use ninja_writer::*;
use quick_xml::{Reader, Writer};
use reqwest::Client;
use walkdir::WalkDir;

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
            ensure_modid(&project).await?;
            write_version_to_java(&project).await?;
            update_build_gradle(&project).await?;
        }
        let build_ninja = project.root.join("build.ninja");
        if !build_ninja.exists() || !self.incremental {
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

        if !self.incremental {
            project.write_mcmod_info().await?;
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

/// Ensures the current modid is the same as declared
async fn ensure_modid(project: &Project) -> io::Result<()> {
    let mcmod = project.mcmod_json().await?;
    let current_modid = find_modid_from_source(project).await?;
    if current_modid == mcmod.modid {
        return Ok(());
    }
    update_modid_in_source_files(project, &current_modid).await?;

    Ok(())
}

/// Find the modid using the source directory
async fn find_modid_from_source(project: &Project) -> io::Result<String> {
    let source_root = project.source_root();
    let mut dir = fs::read_dir(&source_root).await?;
    let mut modid = None;
    while let Some(entry) = dir.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            if modid.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Multiple directories found in {}", source_root.display()),
                ));
            }
            match entry.file_name().to_str() {
                Some(name) => modid = Some(name.to_owned()),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Invalid directory name in {}", source_root.display()),
                    ))
                }
            }
        }
    }
    match modid {
        Some(modid) => Ok(modid),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("No directories found in {}", source_root.display()),
        )),
    }
}

async fn update_modid_in_source_files(project: &Project, old_modid: &str) -> io::Result<()> {
    let new_modid = &project.mcmod_json().await?.modid;
    println!("updating modid from '{old_modid}' to '{new_modid}'");

    let mut old_source_root = project.source_root();
    old_source_root.push(old_modid);
    let mut new_source_root = project.source_root();
    new_source_root.push(new_modid);

    println!(
        "renaming '{}' to '{}'",
        old_source_root.display(),
        new_source_root.display()
    );
    fs::rename(old_source_root, &new_source_root).await?;

    let mut join_set = JoinSet::new();
    let (send, mut recv) = mpsc::channel::<io::Result<PathBuf>>(100);
    let recv_future = async move {
        let mut error = None;
        while let Some(result) = recv.recv().await {
            if error.is_some() {
                continue;
            }
            match result {
                Ok(file) => {
                    println!("updated '{}'", file.display());
                }
                Err(e) => {
                    recv.close();
                    error = Some(e);
                }
            }
        }
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    };
    join_set.spawn(recv_future);
    let old_modid = Arc::from(old_modid);
    let new_modid = Arc::from(new_modid.as_str());
    for entry in WalkDir::new(new_source_root).into_iter() {
        let entry = entry?;
        let name = match entry.file_name().to_str() {
            Some(name) => name,
            None => continue,
        };
        if name.ends_with(".java") {
            let file = entry.path().to_path_buf();
            println!("queueing '{}'", file.display());
            let old_modid = Arc::clone(&old_modid);
            let new_modid = Arc::clone(&new_modid);
            let send = send.clone();
            join_set.spawn(async move {
                let result = update_modid_in(&file, old_modid, new_modid)
                    .await
                    .map(|_| file);
                let _ = send.send(result).await;
                Ok(())
            });
        }
    }

    drop(send);

    while let Some(result) = join_set.join_next().await {
        result??;
    }

    Ok(())
}

async fn update_modid_in(file: &Path, old_modid: Arc<str>, new_modid: Arc<str>) -> io::Result<()> {
    let contents = fs::read_to_string(file).await?;
    let f = File::create(file).await?;
    let mut writer = BufWriter::new(f);
    for line in contents.lines() {
        if line.starts_with("package ") {
            let line = line.replace(old_modid.as_ref(), new_modid.as_ref());
            writer.write_all(line.as_bytes()).await?;
        } else if line.starts_with("import ") {
            let line = line.replace(old_modid.as_ref(), new_modid.as_ref());
            writer.write_all(line.as_bytes()).await?;
        } else {
            writer.write_all(line.as_bytes()).await?;
        }
        writer.write_all(b"\n").await?;
    }
    writer.flush().await?;
    Ok(())
}

async fn write_version_to_java(project: &Project) -> io::Result<()> {
    let mcmod = project.mcmod_json().await?;
    let modid = &mcmod.modid;
    let version = &mcmod.version;
    let group = project.group().await?;
    let group_internal = group.replace('.', "/");

    let mut source_root = project.source_root();
    source_root.push(modid);
    let modinfo_java = source_root.join("ModInfo.java");

    File::create(&modinfo_java)
        .await?
        .write_all(
            format!(
                r###"package {group};

// This file is automatically generated
// Do not edit this file manually

public interface ModInfo {{
    String Id = "{modid}";
    String Version = "{version}";
    String Group = "{group}";
    String GroupInternal = "{group_internal}";
}}

"###
            )
            .as_bytes(),
        )
        .await?;

    source_root.push("coremod");
    if !source_root.exists() {
        return Ok(());
    }

    source_root.push("CoremodInfo.java");

    File::create(&source_root)
        .await?
        .write_all(
            format!(
                r###"package {group}.coremod;

// This file is automatically generated
// Do not edit this file manually

public interface CoremodInfo {{
    String Id = "{modid}";
    String Version = "{version}";
    String Group = "{group}";
    String GroupInternal = "{group_internal}";
    String CoremodGroup = "{group}.coremod";
    String CoremodGroupInternal = "{group_internal}/coremod";
}}

"###
            )
            .as_bytes(),
        )
        .await?;

    Ok(())
}

async fn update_build_gradle(project: &Project) -> io::Result<()> {
    let mcmod = project.mcmod_json().await?;
    let version = &mcmod.version;
    let name = &mcmod.name;
    let archive_base = name.to_ascii_lowercase().replace(" ", "-");
    let group = project.group().await?;

    let mut coremod_root = project.source_root();
    coremod_root.push("coremod");
    let coremod_section = if coremod_root.exists() {
        format!(
            r###"""// coremod
jar {{
    manifest {{
        attributes 'FMLCorePlugin': '{group}.coremod.CoremodMain'
        attributes 'FMLCorePluginContainsFMLMod': 'true'
    }}
}}
// coremod
"###
        )
    } else {
        "".to_owned()
    };

    let mut build_gradle = project.forge_root();
    build_gradle.push("build.gradle");
    let contents = fs::read_to_string(&build_gradle).await?;
    let file = File::create(&build_gradle).await?;
    let mut writer = BufWriter::new(file);
    let mut in_coremod = false;
    for line in contents.lines() {
        let line = if line.starts_with("version") {
            Cow::Owned(format!("version = '{version}'\n"))
        } else if line.starts_with("group") {
            Cow::Owned(format!("group = '{group}'\n"))
        } else if line.starts_with("archivesBaseName") {
            Cow::Owned(format!("archivesBaseName = '{archive_base}'\n"))
        } else if line.starts_with("// coremod") {
            in_coremod = !in_coremod;
            continue;
        } else if line.starts_with("dependencies {") {
            writer.write_all(coremod_section.as_bytes()).await?;
            Cow::Borrowed(line)
        } else {
            Cow::Borrowed(line)
        };
        if !in_coremod {
            writer.write_all(line.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
    }
    writer.flush().await?;
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
    target_root.push(&project.mcmod_json().await?.modid);
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
        println!("downloading '{}'", lib);
        let url = format!("https://cdn.pistonite.org/minecraft/devjars/{lib}");
        let path = libs_root.join(lib);
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
