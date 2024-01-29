use crate::sync::SyncCommand;
use crate::util::{IoResult, Project};

pub async fn run_build(dir: &str) -> IoResult<()> {
    let sync = SyncCommand {
        incremental: false,
        eclipse: true,
    };
    sync.run(dir).await?;
    let project = Project::new_in(dir)?;
    let template_handler = project.mcmod().await?.template.new_handler();
    template_handler.build(&project).await?;
    let output = template_handler.output_dir(&project)?;

    println!();
    println!("the output directory is: {}", output.display());

    Ok(())
}
