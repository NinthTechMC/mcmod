use std::cell::OnceCell;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tokio::fs;

use crate::mcmod::Mcmod;

pub type IoResult<T> = error_stack::Result<T, io::Error>;

macro_rules! cd {
    ($path:expr, $($x:expr),+) => {
        {
            #[allow(unused_mut)]
            let mut path = $path;
            $(
                path.push($x);
            )+
            path
        }
    };
}
pub(crate) use cd;

macro_rules! mkdir {
    ($path:expr) => {
        async {
            let path = $path;
            if !path.exists() {
                fs::create_dir_all(&path).await?;
            }
            Ok::<(), error_stack::Report<tokio::io::Error>>(())
        }
    };
}
pub(crate) use mkdir;

macro_rules! write_file {
    ($path:expr, $content:expr) => {
        async {
            use tokio::io::AsyncWriteExt;
            let path = $path;
            let content = $content;
            tokio::fs::File::create(path)
                .await?
                .write_all(content.as_bytes())
                .await?;
            Ok::<(), error_stack::Report<tokio::io::Error>>(())
        }
    };
}
pub(crate) use write_file;

macro_rules! join_join_set {
    ($join_set:expr) => {
        async {
            let mut join_set = $join_set;
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(result) => result?,
                    Err(e) => Err(tokio::io::Error::from(e))?,
                }
            }
            Ok::<(), error_stack::Report<tokio::io::Error>>(())
        }
    };
}
pub(crate) use join_join_set;

pub fn confirm_yn() -> IoResult<bool> {
    print!("(y/N): ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    match input {
        "y" | "Y" | "yes" | "Yes" => Ok(true),
        "n" | "N" | "no" | "No" => Ok(false),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid input '{}'", input),
        ))?,
    }
}

/// Root of mcmod repo
pub fn tool_root() -> IoResult<PathBuf> {
    let exe = std::env::current_exe()?;
    let root = exe
        .parent() // X/target/profile
        .and_then(|x| x.parent()) // X/target
        .and_then(|x| x.parent()); // X
    match root {
        Some(x) => Ok(x.to_path_buf()),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find root for mcmod. You need the whole repo to run this tool properly, not just the binary",
        ))?,
    }
}

#[derive(Debug)]
pub struct Project {
    /// Root directory of the project
    pub root: PathBuf,
    /// The mcmod.yaml file
    mcmod: OnceCell<Mcmod>,
}

impl Project {
    /// Initialize a new project context in the given directory
    pub fn new_in(dir: &str) -> IoResult<Self> {
        let path = dunce::canonicalize(Path::new(dir))?;
        let mut cur_path = path.as_ref();
        while !path.join("mcmod.yaml").exists() {
            if let Some(parent) = path.parent() {
                cur_path = parent;
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Could not find project root",
                ))?;
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

    /// Get the mcmod.yaml data
    pub async fn mcmod(&self) -> IoResult<&Mcmod> {
        if let Some(x) = self.mcmod.get() {
            return Ok(x);
        }
        let mcmod_path = self.root.join("mcmod.yaml");
        let mcmod = fs::read_to_string(mcmod_path).await?;
        let mut mcmod: Mcmod = match serde_yaml::from_str(&mcmod) {
            Ok(mcmod) => mcmod,
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e))?,
        };
        mcmod.apply_defaults(self).await?;
        Ok(self.mcmod.get_or_init(|| mcmod))
    }

    pub fn source_root(&self) -> PathBuf {
        self.root.join("src")
    }

    /// Detect the group from the src directory
    pub async fn source_group(&self) -> IoResult<String> {
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
                    None => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid source group name",
                    ))?,
                };
                source_group.push_str(name);
            } else {
                break;
            }
            current = entry.path();
        }

        Ok(source_group)
    }

    pub fn target_root(&self) -> PathBuf {
        self.root.join("target")
    }

    pub fn assets_root(&self) -> PathBuf {
        self.root.join("assets")
    }
}
