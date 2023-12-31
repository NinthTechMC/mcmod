use std::io;

use crate::{sync::SyncCommand, util::Project};

pub async fn run_build(dir: &str) -> io::Result<()> {
    let sync = SyncCommand { incremental: false };
    sync.run(dir).await?;
    let project = Project::new_in(dir)?;
    project.run_gradlew(&["build", "deobfJar"]).await?;

    let mut output = project.forge_root();
    output.push("build");
    output.push("libs");

    println!();
    println!("the output directory is: {}", output.display());

    Ok(())
}
