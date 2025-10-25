use rust_vscode_workspace_configurator as lib;
use clap::Parser;
use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    /// Root directory to search for Rust projects. Defaults to current directory.
    #[arg(short, long, value_name = "DIR")]
    root: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let root_dir = args.root.unwrap_or_else(|| std::env::current_dir().unwrap());
    let output_dir = root_dir.clone();

    println!("Searching for Rust projects in: {}", root_dir.display());

    let runnables_lib = lib::discover_runnables(&root_dir)?;

    if runnables_lib.is_empty() {
        println!("No runnables found in {}", root_dir.display());
        return Ok(());
    }

    let runnables = runnables_lib;

    println!("Found {} runnables:", runnables.len());
    for runnable in &runnables {
        println!("  {} ({:?}) in package {}", runnable.name, runnable.runnable_type, runnable.package);
    }

    // Write the workspace file and return its path (library handles everything else)
    let workspace_path = lib::write_workspace_for_root(&output_dir, &runnables, &root_dir)?;
    println!("Created {} with launch configurations in {}", workspace_path.file_name().unwrap().to_string_lossy(), output_dir.display());
    
    Ok(())
}
