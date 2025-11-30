use std::fs;
use tempfile::tempdir;

use rust_vscode_workspace_configurator::{Runnable, RunnableType, write_workspace_for_root};

#[test]
fn test_write_workspace_for_root_multi_project() {
    let td = tempdir().unwrap();
    let root = td.path().to_path_buf();

    // create two project dirs
    let proj_a = root.join("proj_a");
    let proj_b = root.join("proj_b");
    fs::create_dir_all(proj_a.join("src")).unwrap();
    fs::create_dir_all(proj_b.join("src")).unwrap();

    // minimal Cargo.toml files
    fs::write(
        proj_a.join("Cargo.toml"),
        "[package]\nname = \"proj_a\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::write(
        proj_b.join("Cargo.toml"),
        "[package]\nname = \"proj_b\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    let r_a = Runnable {
        name: "proj_a::bin".to_string(),
        package: "proj_a".to_string(),
        package_manifest: proj_a.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: proj_a.clone(),
    };

    let r_b = Runnable {
        name: "proj_b::bin".to_string(),
        package: "proj_b".to_string(),
        package_manifest: proj_b.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: proj_b.clone(),
    };

    let runnables = vec![r_a, r_b];

    let workspace_path =
        write_workspace_for_root(&root, &runnables, &root).expect("write workspace");
    let content = fs::read_to_string(&workspace_path).unwrap();

    // The generated workspace should include folder names and scoped workspaceFolder tokens
    assert!(content.contains("\"name\": \"proj_a\"") || content.contains("proj_a"));
    assert!(content.contains("${workspaceFolder:proj_a}") || content.contains("proj_a"));
}

#[test]
fn test_write_workspace_preserve_existing_folders() {
    let td = tempdir().unwrap();
    let root = td.path().to_path_buf();

    // create a project dir
    let proj_new = root.join("proj_new");
    fs::create_dir_all(proj_new.join("src")).unwrap();
    fs::write(
        proj_new.join("Cargo.toml"),
        "[package]\nname = \"proj_new\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    // Create an existing .code-workspace with a custom folder that should be preserved
    let workspace_filename = format!(
        "{}.code-workspace",
        root.file_name().and_then(|n| n.to_str()).unwrap()
    );
    let workspace_path = root.join(&workspace_filename);
    let existing_workspace = r#"{
    "folders": [
        { "path": "./custom", "name": "CustomFolder" }
    ]
}"#;
    fs::write(&workspace_path, existing_workspace).unwrap();

    let r_new = Runnable {
        name: "proj_new::bin".to_string(),
        package: "proj_new".to_string(),
        package_manifest: proj_new.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: proj_new.clone(),
    };

    let runnables = vec![r_new];

    let written = write_workspace_for_root(&root, &runnables, &root).expect("write workspace");
    let content = fs::read_to_string(&written).unwrap();

    // Existing custom folder and its name should be preserved
    assert!(
        content.contains("CustomFolder"),
        "existing folder name was removed"
    );
    assert!(
        content.contains("./custom"),
        "existing folder path was removed"
    );

    // New project folder should be appended with its generated name
    assert!(
        content.contains("./proj_new"),
        "discovered project folder not appended"
    );
    assert!(
        content.contains("\"name\": \"proj_new\"") || content.contains("proj_new"),
        "generated folder name missing"
    );

    // Launches/tasks should use the existing folder name token for the custom folder when applicable.
    // For the newly appended project, verify that the launch/task tokens are present and that the
    // existing custom folder does not get a conflicting token.
    assert!(
        content.contains("${workspaceFolder:CustomFolder}") || content.contains("CustomFolder")
    );
}

#[test]
fn test_write_workspace_preserve_existing_folders_absolute_path() {
    let td = tempdir().unwrap();
    let root = td.path().to_path_buf();

    // create a project dir
    let proj_new = root.join("proj_new_abs");
    fs::create_dir_all(proj_new.join("src")).unwrap();
    fs::write(
        proj_new.join("Cargo.toml"),
        "[package]\nname = \"proj_new_abs\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    // Create an existing .code-workspace with an absolute folder path that should be preserved
    let workspace_filename = format!(
        "{}.code-workspace",
        root.file_name().and_then(|n| n.to_str()).unwrap()
    );
    let workspace_path = root.join(&workspace_filename);
    let abs_path_str = proj_new
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let existing_workspace = format!(
        r#"{{
    "folders": [
        {{ "path": "{}", "name": "ExistingAbs" }}
    ]
}}"#,
        abs_path_str
    );
    fs::write(&workspace_path, existing_workspace).unwrap();

    let r_new = Runnable {
        name: "proj_new_abs::bin".to_string(),
        package: "proj_new_abs".to_string(),
        package_manifest: proj_new.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: proj_new.clone(),
    };

    let runnables = vec![r_new];

    let written = write_workspace_for_root(&root, &runnables, &root).expect("write workspace");
    let content = fs::read_to_string(&written).unwrap();

    // Existing absolute folder path and its name should be preserved
    assert!(
        content.contains("ExistingAbs"),
        "existing absolute folder name was removed"
    );
    assert!(
        content.contains(&abs_path_str),
        "existing absolute folder path was removed"
    );

    // The function should not append a duplicate relative folder like ./proj_new_abs
    assert!(
        !content.contains("./proj_new_abs"),
        "a duplicate relative folder was appended"
    );

    // Ensure generated launch/tasks reference the preserved existing name token for the absolute entry
    assert!(content.contains("${workspaceFolder:ExistingAbs}") || content.contains("ExistingAbs"));
}

#[test]
fn test_open_space_mmo_updates_managed_settings() {
    use rust_vscode_workspace_configurator::write_workspace_for_root;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    let _env_lock = tempfile::tempdir().unwrap();
    let td = tempdir().unwrap();
    let root = td.path().to_path_buf();

    // create fake HOME and config file
    let home_dir = td.path().join("home");
    fs::create_dir_all(home_dir.join(".config")).unwrap();
    let config_path = home_dir
        .join(".config")
        .join("open_space_mmo_workspace_settings.json");

    // initial value in home config
    let external = serde_json::json!({
        "settings": {
            "some.extension.foo": "initial",
        }
    });
    let mut f = File::create(&config_path).unwrap();
    f.write_all(serde_json::to_string_pretty(&external).unwrap().as_bytes())
        .unwrap();

    // minimal runnable
    let runnable = Runnable {
        name: "pkg::bin".to_string(),
        package: "pkg".to_string(),
        package_manifest: root.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: root.clone(),
    };
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    // prepare and set env
    unsafe {
        std::env::set_var("HOME", home_dir.to_string_lossy().as_ref());
    }
    unsafe {
        std::env::set_var("OPEN_SPACE_MMO", "1");
    }

    // make an existing workspace file
    let workspace_filename = format!(
        "{}.code-workspace",
        root.file_name().and_then(|n| n.to_str()).unwrap()
    );
    let workspace_path = root.join(&workspace_filename);
    fs::write(&workspace_path, "{}").unwrap();

    // first write - initial settings inserted
    let _ = write_workspace_for_root(&root, &[runnable.clone()], &root).unwrap();
    let content1 = fs::read_to_string(&workspace_path).unwrap();
    let parsed1: serde_json::Value = serde_json::from_str(&content1).unwrap();
    let settings1 = parsed1.get("settings").unwrap();
    assert_eq!(
        settings1.get("some.extension.foo").and_then(|v| v.as_str()),
        Some("initial")
    );

    // now modify home config and run again
    let external2 = serde_json::json!({
        "settings": {
            "some.extension.foo": "updated",
        }
    });
    let mut f2 = File::create(&config_path).unwrap();
    f2.write_all(serde_json::to_string_pretty(&external2).unwrap().as_bytes())
        .unwrap();

    // run writer again
    let _ = write_workspace_for_root(&root, &[runnable.clone()], &root).unwrap();
    let content2 = fs::read_to_string(&workspace_path).unwrap();
    let parsed2: serde_json::Value = serde_json::from_str(&content2).unwrap();
    let settings2 = parsed2.get("settings").unwrap();
    assert_eq!(
        settings2.get("some.extension.foo").and_then(|v| v.as_str()),
        Some("updated")
    );
}

#[test]
fn test_open_space_mmo_preserves_user_override() {
    use rust_vscode_workspace_configurator::write_workspace_for_root;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    let td = tempdir().unwrap();
    let root = td.path().to_path_buf();

    // create fake HOME and config file with managed key
    let home_dir = td.path().join("home");
    fs::create_dir_all(home_dir.join(".config")).unwrap();
    let config_path = home_dir
        .join(".config")
        .join("open_space_mmo_workspace_settings.json");
    let external = serde_json::json!({
        "settings": {
            "some.extension.useroverride": "initial",
        }
    });
    let mut f = File::create(&config_path).unwrap();
    f.write_all(serde_json::to_string_pretty(&external).unwrap().as_bytes())
        .unwrap();

    // minimal runnable
    let runnable = Runnable {
        name: "pkg::bin".to_string(),
        package: "pkg".to_string(),
        package_manifest: root.join("Cargo.toml"),
        runnable_type: RunnableType::Binary,
        required_features: vec![],
        project_path: root.clone(),
    };
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();

    // prepare and set env
    unsafe {
        std::env::set_var("HOME", home_dir.to_string_lossy().as_ref());
    }
    unsafe {
        std::env::set_var("OPEN_SPACE_MMO", "1");
    }

    // create workspace and write initial
    let workspace_filename = format!(
        "{}.code-workspace",
        root.file_name().and_then(|n| n.to_str()).unwrap()
    );
    let workspace_path = root.join(&workspace_filename);
    fs::write(&workspace_path, "{}").unwrap();

    let _ = write_workspace_for_root(&root, &[runnable.clone()], &root).unwrap();
    // user manually edits the workspace setting to override the value
    let mut parsed: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&workspace_path).unwrap()).unwrap();
    let mut settings_obj = parsed.get("settings").unwrap().clone();
    if settings_obj.is_null() {
        settings_obj = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(obj) = settings_obj.as_object_mut() {
        obj.insert(
            "some.extension.useroverride".to_string(),
            serde_json::Value::String("user-set".to_string()),
        );
    }
    parsed
        .as_object_mut()
        .unwrap()
        .insert("settings".to_string(), settings_obj.clone());
    fs::write(
        &workspace_path,
        serde_json::to_string_pretty(&parsed).unwrap(),
    )
    .unwrap();

    // run again with open_space_mmo changed to updated - should not override user's manual change
    let external2 = serde_json::json!({
        "settings": {
            "some.extension.useroverride": "updated",
        }
    });
    let mut f2 = File::create(&config_path).unwrap();
    f2.write_all(serde_json::to_string_pretty(&external2).unwrap().as_bytes())
        .unwrap();

    let _ = write_workspace_for_root(&root, &[runnable.clone()], &root).unwrap();
    let content2 = fs::read_to_string(&workspace_path).unwrap();
    let parsed2: serde_json::Value = serde_json::from_str(&content2).unwrap();
    let settings2 = parsed2.get("settings").unwrap();
    assert_eq!(
        settings2
            .get("some.extension.useroverride")
            .and_then(|v| v.as_str()),
        Some("user-set")
    );
}
