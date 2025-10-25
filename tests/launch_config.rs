use std::path::Path;
use rust_vscode_workspace_configurator::{discover_runnables, generate_launch_config};

#[test]
fn test_generate_launch_config_single_project() {
    let fixture_path = Path::new("tests/fixtures/single_project");
    let runnables = discover_runnables(fixture_path).unwrap();
    let cfg = generate_launch_config(&runnables, fixture_path);

    // Expect two configurations (bin and example)
    assert_eq!(cfg.configurations.len(), 2);

    let names: Vec<String> = cfg.configurations.iter().map(|c| c.name.clone()).collect();
    assert!(names.iter().any(|n| n.contains("Bin")));
    assert!(names.iter().any(|n| n.contains("Example")));
}

#[test]
fn test_generate_launch_config_workspace_project() {
    let fixture_path = Path::new("tests/fixtures/workspace_project");
    let runnables = discover_runnables(fixture_path).unwrap();
    let cfg = generate_launch_config(&runnables, fixture_path);

    // We expect two configurations for the workspace members created in fixtures
    assert_eq!(cfg.configurations.len(), 2);
}
