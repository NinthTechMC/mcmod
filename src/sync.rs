use std::borrow::Cow;
use std::io;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use quick_xml::events::{BytesStart, BytesText, Event};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use clap::Parser;
use quick_xml::{Reader, Writer};
use reqwest::Client;

use crate::gradle;
use crate::template::{self, TemplateHandler};
use crate::util::{cd, join_join_set, mkdir, write_file, IoResult, Project};

#[derive(Debug, Parser)]
pub struct SyncCommand {
    /// If syncing incrementally.
    ///
    /// If true, the directory structure and mcmod.yaml is assumed to be the same.
    /// Only updated source and asset files are synced.
    #[arg(short, long)]
    pub incremental: bool,
}

impl SyncCommand {
    pub async fn run(mut self, dir: &str) -> IoResult<()> {
        let project = Project::new_in(dir)?;

        let template_marker = project.target_root().join(".mcmod-template");
        if !template_marker.exists() && !self.incremental {
            println!("forcing non-incremental sync since template has not been setup");
            self.incremental = false;
        }

        if self.incremental {
            sync_source(&project, self.incremental).await?;
            return Ok(());
        }

        let template = &project.mcmod().await?.template;
        let template_handler = template.new_handler();

        let template_name = template.to_string();
        let template_marked = match fs::read_to_string(&template_marker).await {
            Ok(s) => s,
            Err(_) => String::new(),
        };

        let template_updated = template_marked.trim() != template_name;
        if template_updated {
            println!(
                "template is not initialized or has changed. initializing new target directory"
            );
            let target_root = project.target_root();
            if target_root.exists() {
                fs::remove_dir_all(&target_root).await?;
            }
            let templates = template::read_templates().await?;
            let template_def = match templates.get(&template_name) {
                Some(t) => t,
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Template '{}' not found in templates.json. You either specified an invalid template or this is a bug", template_name),
                ))?,
            };
            {
                let status = Command::new("git")
                    .args([
                        "clone",
                        "--branch",
                        &template_def.branch,
                        "--depth",
                        "1",
                        "--recurse-submodules",
                        "--",
                        &template_def.url,
                        target_root.to_str().unwrap(),
                    ])
                    .status()?;

                if !status.success() {
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Failed to clone template",
                    ))?;
                }
            }
        } else {
            println!("using existing target template '{template_name}'");
        }

        println!("syncing gradle properties");
        sync_gradle_properties(template_handler.as_ref(), &project).await?;
        println!("syncing source");
        sync_source(&project, self.incremental).await?;

        println!("syncing metadata");
        sync_metadata(&project).await?;
        println!("syncing libs");
        sync_libs(template_handler.as_ref(), &project).await?;
        println!("syncing mods");
        sync_mods(template_handler.as_ref(), &project).await?;

        if template_updated {
            println!("setting up target template '{template_name}'");
            template_handler.setup_project(&project).await?;
            write_file!(&template_marker, &template_name).await?;
        }

        println!("syncing eclipse");
        sync_eclipse_workspace(template_handler.as_ref(), &project).await?;

        Ok(())
    }
}

async fn sync_gradle_properties(handler: &dyn TemplateHandler, project: &Project) -> IoResult<()> {
    println!("updating gradle.properties");
    let mut properties = handler.make_gradle_properties(project).await?;
    for (k, v) in project.mcmod().await?.gradle_overrides.iter() {
        properties.insert(k.clone(), v.clone());
    }
    let gradle_properties = cd!(project.target_root(), "gradle.properties");
    gradle::merge_properties(&gradle_properties, properties).await?;
    Ok(())
}

async fn sync_source(project: &Project, incremental: bool) -> IoResult<()> {
    let build_ninja = project.root.join("build.ninja");
    if !build_ninja.exists() || !incremental {
        let mut forge_source_root = project.target_root();
        forge_source_root.push("src");
        if forge_source_root.exists() {
            fs::remove_dir_all(&forge_source_root).await?;
        }
        let ninja_file = project
            .mcmod()
            .await?
            .create_build_ninja(&project.root, &project.target_root())
            .await?;
        write_file!(&build_ninja, ninja_file).await?;
    }

    let result = Command::new("ninja").current_dir(&project.root).status()?;

    if !result.success() {
        Err(io::Error::new(io::ErrorKind::Other, "ninja failed"))?;
    }
    Ok(())
}

async fn sync_metadata(project: &Project) -> IoResult<()> {
    let mcmod = project.mcmod().await?;
    let resource_path = cd!(project.target_root(), "src", "main", "resources");
    mkdir!(&resource_path).await?;
    let mcmod_info_future = async {
        let info_str = mcmod.create_mcmod_info()?;
        write_file!(resource_path.join("mcmod.info"), info_str).await
    };
    let pack_mcmeta_future = async {
        let pack_str = mcmod.create_pack_mcmeta()?;
        write_file!(resource_path.join("pack.mcmeta"), pack_str).await
    };
    let (r1, r2) = tokio::join!(mcmod_info_future, pack_mcmeta_future);
    r1?;
    r2?;
    Ok(())
}

async fn sync_libs(template_handler: &dyn TemplateHandler, project: &Project) -> IoResult<()> {
    let libs_root = template_handler.libs_dir(project)?;
    let libs = &project.mcmod().await?.libs;
    let cdn_url_prefix = "https://cdn.pistonite.org/minecraft/devjars/";
    sync_downloads(&libs_root, libs, cdn_url_prefix).await?;
    Ok(())
}

async fn sync_mods(template_handler: &dyn TemplateHandler, project: &Project) -> IoResult<()> {
    let mods_root = cd!(template_handler.run_dir(project)?, "mods");
    let mods = &project.mcmod().await?.mods;
    let cdn_url_prefix = "https://cdn.pistonite.org/minecraft/jars/";
    sync_downloads(&mods_root, mods, cdn_url_prefix).await?;
    Ok(())
}

async fn sync_downloads(libs_root: &Path, libs: &[String], cdn_url_prefix: &str) -> IoResult<()> {
    let mut needs_download = libs.iter().map(|lib| lib.as_str()).collect::<Vec<_>>();
    mkdir!(libs_root).await?;
    let mut dir = fs::read_dir(&libs_root).await?;
    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        let name = match file_name.to_str() {
            Some(name) => name,
            None => continue,
        };
        match needs_download.iter().position(|lib| {
            if lib.starts_with("http") || lib.starts_with("./") {
                Path::new(lib)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s == name)
                    .unwrap_or(false)
            } else {
                lib == &name
            }
        }) {
            Some(i) => {
                // up to date
                needs_download.swap_remove(i);
            }
            None => {
                let path = entry.path();
                println!("removing '{}'", path.display());
                if path.is_dir() {
                    fs::remove_dir_all(path).await?;
                } else {
                    fs::remove_file(path).await?;
                }
            }
        }
    }
    let mut join_set = JoinSet::new();
    let (send, mut recv) = mpsc::channel::<IoResult<String>>(100);
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
        if lib.starts_with("./") {
            let file_name = match Path::new(lib).file_name() {
                Some(name) => name,
                None => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Cannot find file name in path '{lib}'"),
                ))?,
            };
            println!("copying '{lib}'");
            let path = libs_root.join(file_name);
            fs::copy(lib, path).await?;
            continue;
        }
        let (url, path) = if lib.starts_with("http") {
            let url = lib.to_owned();
            let file_name = match Path::new(&url).file_name() {
                Some(name) => name,
                None => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Cannot find file name in url '{url}'"),
                ))?,
            };
            let path = libs_root.join(file_name);
            (url, path)
        } else {
            // let url = format!("https://cdn.pistonite.org/minecraft/devjars/{lib}");
            let url = format!("{cdn_url_prefix}{lib}");
            let path = libs_root.join(lib);
            (url, path)
        };
        println!("downloading '{url}'");
        let client = Arc::clone(&client);
        let send = send.clone();
        join_set.spawn(async move {
            let result = download_binary(client, &url, &path).await.map(|_| url);
            let _ = send.send(result).await;
            Ok(())
        });
    }
    drop(send);
    join_join_set!(join_set).await?;
    Ok(())
}

async fn download_binary(client: Arc<Client>, url: &str, path: &Path) -> IoResult<()> {
    let bytes_result = async { client.get(url).send().await?.bytes().await }.await;

    let bytes = match bytes_result {
        Ok(response) => response,
        Err(e) => Err(io::Error::new(io::ErrorKind::Other, e))?,
    };

    File::create(path).await?.write_all(&bytes).await?;

    Ok(())
}

async fn sync_eclipse_workspace(
    template_handler: &dyn TemplateHandler,
    project: &Project,
) -> IoResult<()> {
    template_handler.setup_eclipse(project).await?;
    let output_file = project.root.join(".classpath");
    let writer = std::io::BufWriter::new(std::fs::File::create(&output_file)?);
    let classpath_file = project.target_root().join(".classpath");
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
                                    // if assets don't exist, add forge prefix
                                    let assets_dir = project.assets_root();
                                    let exists = assets_dir.exists();
                                    if exists {
                                        attr.value = Cow::Borrowed(b"assets");
                                    } else {
                                        attr.value = Cow::Borrowed(b"target/src/main/resources");
                                    }
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
        Err(io::Error::new(io::ErrorKind::InvalidData, e))?;
    }

    fs::remove_file(classpath_file).await?;

    let output_file = project.root.join(".project");
    let project_name = match project.root.file_name().and_then(|s| s.to_str()) {
        Some(name) => name,
        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Cannot determine project name from root path",
        ))?,
    };
    let writer = std::io::BufWriter::new(std::fs::File::create(&output_file)?);
    let project_file = project.target_root().join(".project");
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
                    if !found_name && level == 1 && e.name().as_ref() == b"name" {
                        found_name = true;
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
                        writer.write_event(Event::Text(BytesText::new(project_name)))?;
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
        Err(io::Error::new(io::ErrorKind::InvalidData, e))?;
    }

    Ok(())
}
