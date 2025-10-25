use rust_vscode_workspace_configurator::discover_runnables;
use rust_vscode_workspace_configurator::RunnableType;
use std::path::Path;

#[test]
fn test_discover_runnables_single_project() {
    let fixture_path = Path::new("tests/fixtures/single_project");

    let runnables = discover_runnables(fixture_path).unwrap();

    assert_eq!(runnables.len(), 2);

    // Check binary
    let binary = runnables.iter().find(|r| r.runnable_type == RunnableType::Binary).unwrap();
    assert_eq!(binary.name, "test-project::test-bin");
    assert_eq!(binary.package, "test-project");
    assert_eq!(binary.runnable_type, RunnableType::Binary);

    // Check example
    let example = runnables.iter().find(|r| r.runnable_type == RunnableType::Example).unwrap();
    assert_eq!(example.name, "test-project::test-example (example)");
    assert_eq!(example.package, "test-project");
    assert_eq!(example.runnable_type, RunnableType::Example);
}

#[test]
fn test_discover_runnables_workspace_project() {
    let fixture_path = Path::new("tests/fixtures/workspace_project");

    let runnables = discover_runnables(fixture_path).unwrap();

    assert_eq!(runnables.len(), 2);

    // Check member1 binary
    let member1_bin = runnables.iter().find(|r| r.name.contains("member1")).unwrap();
    assert_eq!(member1_bin.name, "member1::member1-bin");
    assert_eq!(member1_bin.package, "member1");
    assert_eq!(member1_bin.runnable_type, RunnableType::Binary);

    // Check member2 example
    let member2_example = runnables.iter().find(|r| r.name.contains("member2")).unwrap();
    assert_eq!(member2_example.name, "member2::member2-example (example)");
    assert_eq!(member2_example.package, "member2");
    assert_eq!(member2_example.runnable_type, RunnableType::Example);
}

#[test]
fn test_discover_runnables_multi_project() {
    let fixture_path = Path::new("tests/fixtures/multi_project");

    let runnables = discover_runnables(fixture_path).unwrap();

    assert_eq!(runnables.len(), 2);

    // Check project-a binary
    let project_a_bin = runnables.iter().find(|r| r.name.contains("project-a")).unwrap();
    assert_eq!(project_a_bin.name, "project-a::project-a-bin");
    assert_eq!(project_a_bin.package, "project-a");
    assert_eq!(project_a_bin.runnable_type, RunnableType::Binary);

    // Check project-b example
    let project_b_example = runnables.iter().find(|r| r.name.contains("project-b")).unwrap();
    assert_eq!(project_b_example.name, "project-b::project-b-example (example)");
    assert_eq!(project_b_example.package, "project-b");
    assert_eq!(project_b_example.runnable_type, RunnableType::Example);
}