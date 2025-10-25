use cargo_metadata::{CargoOpt, MetadataCommand, TargetKind};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use std::fs;
use regex::Regex;
use std::error::Error;
use serde::{Deserialize, Serialize};
use serde_json;
use atty::Stream;
use pathdiff;

/// The type of runnable target.
/// Type of a runnable target discovered in a package.
///
/// - `Binary` indicates a normal binary target.
/// - `Example` indicates a packaged example target.
#[derive(Debug, Clone, PartialEq)]
pub enum RunnableType {
    Binary,
    Example,
}

/// Represents a runnable target (binary or example) in a Rust project.
/// Representation of a single runnable target (binary or example).
///
/// Fields:
/// - `name`: displayed name, typically "<package>::<target>" or with " (example)" suffix for examples.
/// - `package`: the Cargo package name.
/// - `package_manifest`: path to the package's Cargo.toml.
/// - `runnable_type`: distinguishes binary vs example.
/// - `required_features`: features required by the target.
/// - `project_path`: filesystem path of the project that contains the target.
#[derive(Debug, Clone)]
pub struct Runnable {
    pub name: String,
    pub package: String,
    pub package_manifest: PathBuf,
    pub runnable_type: RunnableType,
    pub required_features: Vec<String>,
    pub project_path: PathBuf,
}

/// Discover runnables (binaries and examples) under `root_dir`.
pub fn discover_runnables(root_dir: &Path) -> Result<Vec<Runnable>, Box<dyn std::error::Error>> {
    let mut runnables = Vec::new();
    let mut found_projects = Vec::new();

    // First confirm this root directory itself is a Rust project
    let manifest_path = root_dir.join("Cargo.toml");
    if manifest_path.exists() {
        found_projects.push(root_dir.to_path_buf());
    } else {
        find_rust_projects_recursive(root_dir, &mut found_projects)?;

        if found_projects.is_empty() {
            return Err(format!("No Rust projects (Cargo.toml files) found in {}", root_dir.display()).into());
        }
    }

    for project_path in found_projects {
        let manifest_path = project_path.join("Cargo.toml");
        let metadata = match MetadataCommand::new()
            .manifest_path(&manifest_path)
            .features(CargoOpt::AllFeatures)
            .exec()
        {
            Ok(metadata) => metadata,
            Err(e) => {
                eprintln!("Warning: Failed to read metadata for {}: {}. Skipping this path.", manifest_path.display(), e);
                continue;
            }
        };

        let canonical_project_path = project_path.canonicalize().unwrap_or_else(|_| project_path.clone());

        // Handle both workspace and single package cases
        let packages_to_process: Vec<&cargo_metadata::Package> = if metadata.workspace_members.is_empty() {
            // Single package project - find the package that matches this manifest path
            let canonical_manifest = manifest_path.canonicalize().unwrap_or(manifest_path.clone());

            match metadata.packages.iter().find(|p| {
                let pkg_manifest_canonical = p.manifest_path.as_std_path().canonicalize()
                    .unwrap_or_else(|_| p.manifest_path.as_std_path().to_path_buf());
                pkg_manifest_canonical == canonical_manifest
            }) {
                Some(package) => vec![package],
                None => {
                    eprintln!("Warning: Could not find package for manifest {}. Skipping this path.", manifest_path.display());
                    continue;
                }
            }
        } else {
            // Workspace project - process all workspace members that are in this project directory
            metadata
                .packages
                .iter()
                .filter(|p| {
                    let pkg_manifest_dir = p.manifest_path.parent().unwrap_or(&p.manifest_path);
                    let pkg_canonical_dir = pkg_manifest_dir.as_std_path().canonicalize()
                        .unwrap_or_else(|_| pkg_manifest_dir.as_std_path().to_path_buf());
                    pkg_canonical_dir.starts_with(&canonical_project_path)
                })
                .collect()
        };

        if packages_to_process.is_empty() {
            eprintln!("Warning: No packages found for project {}. Nothing to run.", project_path.display());
            continue;
        }

        for package in packages_to_process {
            for target in &package.targets {
                if target.kind.contains(&TargetKind::Bin) {
                    runnables.push(Runnable {
                        name: format!("{}::{}", package.name, target.name),
                        package: package.name.to_string(),
                        package_manifest: package.manifest_path.as_std_path().to_path_buf(),
                        runnable_type: RunnableType::Binary,
                        required_features: target.required_features.clone(),
                        project_path: project_path.clone(),
                    });
                }

                if target.kind.contains(&TargetKind::Example) {
                    runnables.push(Runnable {
                        name: format!("{}::{} (example)", package.name, target.name),
                        package: package.name.to_string(),
                        package_manifest: package.manifest_path.as_std_path().to_path_buf(),
                        runnable_type: RunnableType::Example,
                        required_features: target.required_features.clone(),
                        project_path: project_path.clone(),
                    });
                }
            }
        }
    }

    Ok(runnables)
}

fn find_rust_projects_recursive(dir: &Path, projects: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if !dir.is_dir() {
        print!("{} is not a directory though we thought it might be.", dir.display());
        return Ok(());
    }

    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.exists() {
        projects.push(dir.to_path_buf());
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
            }

            find_rust_projects_recursive(&path, projects)?;
        }
    }

    Ok(())
}

/// Launch configuration structures
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LaunchConfig {
    pub version: String,
    pub configurations: Vec<Configuration>,
}

/// Represents a single launch configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Configuration {
    pub name: String,
    #[serde(rename = "type")]
    pub config_type: String,
    pub request: String,
    pub cwd: String,
    pub env: EnvVars,
    pub cargo: CargoConfig,
    pub args: Vec<String>,
}

/// Environment variables for the launch configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnvVars {
    #[serde(rename = "BEVY_ASSET_ROOT")]
    pub bevy_asset_root: String,
}

/// Cargo command configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CargoConfig {
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Represents a VSCode workspace launch configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkspaceLaunchConfig {
    pub version: String,
    pub configurations: Vec<Configuration>,
}

/// Represents a VSCode workspace file.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorkspaceFile {
    pub folders: Vec<WorkspaceFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch: Option<WorkspaceLaunchConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<WorkspaceTasks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

/// Represents VSCode workspace tasks.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorkspaceTasks {
    pub version: String,
    pub tasks: Vec<Task>,
}

/// Represents a single VSCode task.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Task {
    pub label: String,
    #[serde(rename = "type")]
    pub task_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<TaskGroup>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "problemMatcher")]
    pub problem_matcher: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "runOptions")]
    pub run_options: Option<serde_json::Value>,
}

/// Represents a task group, which can be a simple string or a complex object.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum TaskGroup {
    Simple(String),
    Complex(TaskGroupComplex),
}

/// Represents a complex task group with additional properties.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TaskGroupComplex {
    pub kind: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

/// Represents a workspace folder.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WorkspaceFolder {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Generate launch configurations for the provided runnables.
/// Generate a VS Code `launch.json`-style structure for the provided runnables.
/// The returned `LaunchConfig` contains configurations which use scoped `${workspaceFolder:<name>}`
/// tokens when multiple folders are present. Callers can serialize this via `serde_json`.
pub fn generate_launch_config(runnables: &[Runnable], root_dir: &Path) -> LaunchConfig {
    let mut configurations = Vec::new();

    // Build a list of unique project paths and a stable unique folder name for each.
    let mut project_paths: Vec<PathBuf> = runnables.iter().map(|r| r.project_path.clone()).collect();
    project_paths.sort();
    project_paths.dedup();

    let mut name_map: HashMap<PathBuf, String> = HashMap::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for project_path in &project_paths {
        let mut candidate = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string();
        if candidate == "." || candidate.is_empty() {
            candidate = "root".to_string();
        }
        // sanitize: replace spaces with underscore
        candidate = candidate.replace(' ', "_");

        let original = candidate.clone();
        let mut suffix = 1;
        while used_names.contains(&candidate) {
            candidate = format!("{}-{}", original, suffix);
            suffix += 1;
        }
        used_names.insert(candidate.clone());
        name_map.insert(project_path.clone(), candidate);
    }

    for runnable in runnables {
        let relative_path = match pathdiff::diff_paths(&runnable.project_path, root_dir) {
            Some(path) => path,
            None => runnable.project_path.clone(),
        };

        // Determine the folder token for this runnable's project (scoped workspaceFolder)
        let folder_name = name_map.get(&runnable.project_path)
            .map(|s| s.clone())
            .unwrap_or_else(|| "root".to_string());
        let folder_token = format!("${{workspaceFolder:{}}}", folder_name);

        let cwd = folder_token.clone();
        let manifest_path_arg = format!("{}/Cargo.toml", folder_token);
        let target_dir_arg = format!("{}/target", folder_token);

        let project_manifest = if relative_path == Path::new("") || relative_path == Path::new(".") {
            root_dir.join("Cargo.toml").canonicalize().unwrap_or(root_dir.join("Cargo.toml"))
        } else {
            root_dir.join(&relative_path).join("Cargo.toml").canonicalize().unwrap_or(root_dir.join(&relative_path).join("Cargo.toml"))
        };

        match runnable.runnable_type {
            RunnableType::Binary => {
                let binary_name = runnable.name.split("::").last().unwrap_or(&runnable.name);
                let mut args = vec!["run".to_string(), "--bin".to_string(), binary_name.to_string()];

                let pkg_manifest = runnable.package_manifest.canonicalize().unwrap_or(runnable.package_manifest.clone());
                if pkg_manifest != project_manifest {
                    args.push("--package".to_string());
                    args.push(runnable.package.clone());
                }

                if !runnable.required_features.is_empty() {
                    let feats = runnable.required_features.join(",");
                    args.push("--features".to_string());
                    args.push(feats);
                }

                args.push("--target-dir".to_string());
                args.push(target_dir_arg.clone());
                args.push("--manifest-path".to_string());
                args.push(manifest_path_arg.clone());

                configurations.push(Configuration {
                    name: format!("{} (Bin)", binary_name),
                    config_type: "lldb".to_string(),
                    request: "launch".to_string(),
                    cwd: cwd.clone(),
                    env: EnvVars { bevy_asset_root: cwd.clone() },
                    cargo: CargoConfig { args, cwd: Some(cwd.clone()) },
                    args: vec![],
                });
            }
            RunnableType::Example => {
                let example_name = runnable.name.split("::").nth(1)
                    .and_then(|s| s.strip_suffix(" (example)"))
                    .unwrap_or(&runnable.name);

                let mut args = vec!["run".to_string(), "--example".to_string(), example_name.to_string()];

                let pkg_manifest = runnable.package_manifest.canonicalize().unwrap_or(runnable.package_manifest.clone());
                if pkg_manifest != project_manifest {
                    args.push("--package".to_string());
                    args.push(runnable.package.clone());
                }

                if !runnable.required_features.is_empty() {
                    let feats = runnable.required_features.join(",");
                    args.push("--features".to_string());
                    args.push(feats);
                }

                args.push("--target-dir".to_string());
                args.push(target_dir_arg.clone());
                args.push("--manifest-path".to_string());
                args.push(manifest_path_arg.clone());

                configurations.push(Configuration {
                    name: format!("{} (Example)", example_name),
                    config_type: "lldb".to_string(),
                    request: "launch".to_string(),
                    cwd: cwd.clone(),
                    env: EnvVars { bevy_asset_root: cwd.clone() },
                    cargo: CargoConfig { args, cwd: Some(cwd.clone()) },
                    args: vec![],
                });
            }
        }
    }

    LaunchConfig {
        version: "0.2.0".to_string(),
        configurations,
    }
}

/// Create a reusable Configuration object. This mirrors the helper previously in the binary.
/// Create a single debug `Configuration` for a runnable.
///
/// This helper centralizes how cargo args, manifest-path and target-dir are composed so callers
/// (including the binary) can reuse the same logic. Example:
///
/// ```rust
/// # use rust_vscode_workspace_configurator::{create_configuration, Runnable, RunnableType, Configuration};
/// # use std::path::PathBuf;
/// # let runnable = Runnable { name: "pkg::bin".into(), package: "pkg".into(), package_manifest: PathBuf::from("/tmp/pkg/Cargo.toml"), runnable_type: RunnableType::Binary, required_features: vec![], project_path: PathBuf::from("/tmp/pkg") };
/// let cfg = create_configuration("Run (Bin)".into(), "${workspaceFolder}".into(), "${workspaceFolder}/Cargo.toml".into(), "${workspaceFolder}/target".into(), PathBuf::from("/tmp/pkg/Cargo.toml"), &runnable, vec!["--bin".into(), "bin".into()]);
/// assert_eq!(cfg.config_type, "lldb");
/// ```
pub fn create_configuration(
    name: String,
    cwd: String,
    manifest_path_arg: String,
    target_dir_arg: String,
    project_manifest: PathBuf,
    runnable: &Runnable,
    type_args: Vec<String>,
) -> Configuration {
    let mut args = vec!["run".to_string()];
    args.extend(type_args.into_iter());

    let pkg_manifest = runnable.package_manifest.canonicalize().unwrap_or(runnable.package_manifest.clone());
    if pkg_manifest != project_manifest {
        args.push("--package".to_string());
        args.push(runnable.package.clone());
    }

    if !runnable.required_features.is_empty() {
        let feats = runnable.required_features.join(",");
        args.push("--features".to_string());
        args.push(feats);
    }

    args.push("--target-dir".to_string());
    args.push(target_dir_arg);

    args.push("--manifest-path".to_string());
    args.push(manifest_path_arg);

    Configuration {
        name,
        config_type: "lldb".to_string(),
        request: "launch".to_string(),
        cwd: cwd.clone(),
        env: EnvVars { bevy_asset_root: cwd.clone() },
        cargo: CargoConfig { args, cwd: Some(cwd) },
        args: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn acquire_env_lock() -> MutexGuard<'static, ()> {
        ENV_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    // Helper to temporarily set environment variables and restore them on drop.
    struct EnvRestore {
        entries: Vec<(String, Option<String>)>,
    }

    impl EnvRestore {
        fn new() -> Self { EnvRestore { entries: Vec::new() } }
        fn set(&mut self, k: &str, v: &str) {
            let prev = std::env::var(k).ok();
            self.entries.push((k.to_string(), prev));
            unsafe { std::env::set_var(k, v); }
        }
        fn remove(&mut self, k: &str) {
            let prev = std::env::var(k).ok();
            self.entries.push((k.to_string(), prev));
            unsafe { std::env::remove_var(k); }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (k, prev) in self.entries.drain(..).rev() {
                match prev {
                    Some(val) => unsafe { std::env::set_var(&k, &val); },
                    None => unsafe { std::env::remove_var(&k); },
                }
            }
        }
    }

    #[test]
    fn test_create_configuration_basic() {
        let runnable = Runnable {
            name: "pkg::bin".to_string(),
            package: "pkg".to_string(),
            package_manifest: PathBuf::from("/tmp/pkg/Cargo.toml"),
            runnable_type: RunnableType::Binary,
            required_features: vec!["feat1".to_string()],
            project_path: PathBuf::from("/tmp/pkg"),
        };

        let cfg = create_configuration(
            "Run (Bin)".to_string(),
            "${workspaceFolder}".to_string(),
            "${workspaceFolder}/Cargo.toml".to_string(),
            "${workspaceFolder}/target".to_string(),
            PathBuf::from("/tmp/pkg/Cargo.toml"),
            &runnable,
            vec!["--bin".to_string(), "bin".to_string()],
        );

        assert_eq!(cfg.name, "Run (Bin)");
        assert_eq!(cfg.config_type, "lldb");
        // cargo args should contain --bin and --features and --manifest-path/--target-dir
        let args = &cfg.cargo.args;
        assert!(args.contains(&"--bin".to_string()));
        assert!(args.contains(&"--features".to_string()));
        assert!(args.iter().any(|a| a.contains("--manifest-path") || a.contains("--target-dir") || a.contains("--package")));
        assert_eq!(cfg.cwd, "${workspaceFolder}".to_string());
    }

    #[test]
    fn test_open_space_mmo_merge_from_file() {
        // Create a temporary directory to act as the workspace root / output dir.
        // Serialize environment-modifying tests to avoid races when cargo runs tests in parallel.
        let _env_lock = acquire_env_lock();
        let td = tempdir().expect("tempdir");
        let root = td.path().to_path_buf();

        // Write an external settings file that would be merged when OPEN_SPACE_MMO=1
        let external = serde_json::json!({
            "git.autoRepositoryDetection": false,
            "git.ignoredRepositories": ["./bevy_0.17_2"],
            "editor.inlayHints.enabled": "offUnlessPressed"
        });

        // Write the config into a fake HOME/.config/open_space_mmo_workspace_settings.json
        let home_dir = td.path().join("home");
        fs::create_dir_all(home_dir.join(".config")).expect("create home .config");
        let config_path = home_dir.join(".config").join("open_space_mmo_workspace_settings.json");
        let mut f = File::create(&config_path).expect("create config file");
        f.write_all(serde_json::to_string_pretty(&external).unwrap().as_bytes()).expect("write config");

        // Create a pre-existing workspace file that already contains one setting we expect to preserve.
        let workspace_filename = generate_workspace_filename(&root);
        let workspace_path = root.join(&workspace_filename);
        let pre_existing = serde_json::json!({
            "folders": [],
            "settings": {
                "git.autoRepositoryDetection": true
            }
        });
        let mut wf = File::create(&workspace_path).expect("create workspace");
        wf.write_all(serde_json::to_string_pretty(&pre_existing).unwrap().as_bytes()).expect("write workspace");

        // Create a minimal runnable so the function produces launches/tasks.
        let runnable = Runnable {
            name: "pkg::bin".to_string(),
            package: "pkg".to_string(),
            package_manifest: root.join("Cargo.toml"),
            runnable_type: RunnableType::Binary,
            required_features: vec![],
            project_path: root.clone(),
        };

        // Ensure there is a Cargo.toml (not strictly necessary but keeps paths realistic).
        let _ = File::create(root.join("Cargo.toml")).expect("create cargo toml");

    // Set HOME to point at our fake home and enable merging via OPEN_SPACE_MMO.
    let mut env_guard = EnvRestore::new();
    env_guard.set("HOME", home_dir.to_string_lossy().as_ref());
    env_guard.set("OPEN_SPACE_MMO", "1");

        // Run the writer which should merge external settings into the existing workspace file
        let written = write_workspace_for_root(&root, &[runnable], &root).expect("write workspace");

        // Load the written workspace and assert merging behavior.
        let content = fs::read_to_string(written).expect("read written workspace");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");
        let settings = parsed.get("settings").expect("settings present");

        // Existing key should be preserved (true) and other keys added from external config
        assert_eq!(settings.get("git.autoRepositoryDetection").and_then(|v| v.as_bool()), Some(true));
        assert!(settings.get("git.ignoredRepositories").is_some());
        assert_eq!(settings.get("editor.inlayHints.enabled").and_then(|v| v.as_str()), Some("offUnlessPressed"));
    }

    #[test]
    fn test_no_env_no_workspace_settings_file() {
        // Ensure OPEN_SPACE_MMO is not set and there are no workspace_settings.json files
        // Serialize environment-modifying tests to avoid races when cargo runs tests in parallel.
        let _env_lock = acquire_env_lock();
        unsafe { std::env::remove_var("OPEN_SPACE_MMO"); }

        let td = tempdir().expect("tempdir");
        let root = td.path().to_path_buf();

        // Create a pre-existing workspace file with a setting that should be preserved
        let workspace_filename = generate_workspace_filename(&root);
        let workspace_path = root.join(&workspace_filename);
        let pre_existing = serde_json::json!({
            "folders": [],
            "settings": {
                "git.autoRepositoryDetection": true
            }
        });
        let mut wf = File::create(&workspace_path).expect("create workspace");
        wf.write_all(serde_json::to_string_pretty(&pre_existing).unwrap().as_bytes()).expect("write workspace");

        // Create a minimal runnable
        let runnable = Runnable {
            name: "pkg::bin".to_string(),
            package: "pkg".to_string(),
            package_manifest: root.join("Cargo.toml"),
            runnable_type: RunnableType::Binary,
            required_features: vec![],
            project_path: root.clone(),
        };

        // Ensure there is a Cargo.toml
        let _ = File::create(root.join("Cargo.toml")).expect("create cargo toml");

        // Ensure HOME is a clean empty directory so no user config is picked up.
        let home_dir = td.path().join("home_no_config");
        fs::create_dir_all(&home_dir).expect("create fake home");
        let mut env_guard = EnvRestore::new();
        env_guard.set("HOME", home_dir.to_string_lossy().as_ref());
        // Ensure OPEN_SPACE_MMO is not set
        env_guard.remove("OPEN_SPACE_MMO");

        // Do not create any *workspace_settings.json file in the root

        // Run the writer
        let written = write_workspace_for_root(&root, &[runnable], &root).expect("write workspace");

        // Load and verify existing setting preserved and nothing new added
        let content = fs::read_to_string(written).expect("read written workspace");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");
        let settings = parsed.get("settings").expect("settings present");

        // Existing key preserved
        assert_eq!(settings.get("git.autoRepositoryDetection").and_then(|v| v.as_bool()), Some(true));
        // No new keys were added
        assert!(settings.get("editor.inlayHints.enabled").is_none());
    }

    #[test]
    fn test_merge_from_workspace_file_with_settings_key() {
        // Serialize environment-modifying tests to avoid races when cargo runs tests in parallel.
        let _env_lock = acquire_env_lock();
        let td = tempdir().expect("tempdir");
        let root = td.path().to_path_buf();

        // Write an external config that looks like a workspace file with a "settings" key
        let external = serde_json::json!({
            "folders": [],
            "settings": {
                "git.autoRepositoryDetection": false,
                "git.ignoredRepositories": ["./bevy_0.17_2"],
                "editor.inlayHints.enabled": "offUnlessPressed"
            }
        });

        // Write the config into a workspace_settings.json file
        let config_path = root.join("workspace_settings.json");
        let mut f = File::create(&config_path).expect("create config file");
        f.write_all(serde_json::to_string_pretty(&external).unwrap().as_bytes()).expect("write config");

        // Create a pre-existing workspace file that already contains one setting we expect to preserve.
        let workspace_filename = generate_workspace_filename(&root);
        let workspace_path = root.join(&workspace_filename);
        let pre_existing = serde_json::json!({
            "folders": [],
            "settings": {
                "git.autoRepositoryDetection": true
            }
        });
        let mut wf = File::create(&workspace_path).expect("create workspace");
        wf.write_all(serde_json::to_string_pretty(&pre_existing).unwrap().as_bytes()).expect("write workspace");

        // Create a minimal runnable so the function produces launches/tasks.
        let runnable = Runnable {
            name: "pkg::bin".to_string(),
            package: "pkg".to_string(),
            package_manifest: root.join("Cargo.toml"),
            runnable_type: RunnableType::Binary,
            required_features: vec![],
            project_path: root.clone(),
        };

        // Ensure there is a Cargo.toml
        let _ = File::create(root.join("Cargo.toml")).expect("create cargo toml");

        // Run the writer which should merge the settings from the external workspace file
        let written = write_workspace_for_root(&root, &[runnable], &root).expect("write workspace");

        // Load the written workspace and assert merging behavior.
        let content = fs::read_to_string(written).expect("read written workspace");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("parse json");
        let settings = parsed.get("settings").expect("settings present");

        // Existing key should be preserved (true) and other keys added from external config
        assert_eq!(settings.get("git.autoRepositoryDetection").and_then(|v| v.as_bool()), Some(true));
        assert!(settings.get("git.ignoredRepositories").is_some());
        assert_eq!(settings.get("editor.inlayHints.enabled").and_then(|v| v.as_str()), Some("offUnlessPressed"));
    }
}

fn generate_workspace_filename(root_dir: &Path) -> String {
    let root_name = root_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("rust-projects");
    format!("{}.code-workspace", root_name)
}

fn generate_workspace_name(root_dir: &Path, project_paths: &[PathBuf]) -> String {
    if project_paths.len() == 1 {
        if let Some(project_name) = project_paths[0].file_name().and_then(|n| n.to_str()) {
            return format!("{} (Rust)", project_name);
        }
    }

    let root_name = root_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Rust Projects");

    if project_paths.len() > 1 {
        format!("{} ({} Rust Projects)", root_name, project_paths.len())
    } else {
        format!("{} (Rust)", root_name)
    }
}

fn generate_default_tasks_internal(project_paths: &[PathBuf], name_map: &HashMap<PathBuf, String>) -> WorkspaceTasks {
    let mut tasks = Vec::new();

    for project_path in project_paths {
        let project_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        let folder_name = name_map.get(project_path).cloned().unwrap_or_else(|| "root".to_string());
        let folder_token = format!("${{workspaceFolder:{}}}", folder_name);
        let cwd = Some(folder_token);

        let build_label = if project_paths.len() == 1 {
            "Build Project".to_string()
        } else {
            format!("Build {}", project_name)
        };

        tasks.push(Task {
            label: build_label,
            task_type: "shell".to_string(),
            command: "cargo".to_string(),
            args: Some(vec!["build".to_string()]),
            cwd: cwd.clone(),
            group: Some(TaskGroup::Complex(TaskGroupComplex {
                kind: "build".to_string(),
                is_default: project_paths.len() == 1,
            })),
            presentation: Some(serde_json::json!({
                "echo": true,
                "reveal": "always",
                "focus": false,
                "panel": "shared"
            })),
            problem_matcher: Some(vec!["$rustc".to_string()]),
            run_options: None,
        });

        let run_label = if project_paths.len() == 1 {
            "Run Project".to_string()
        } else {
            format!("Run {}", project_name)
        };

        tasks.push(Task {
            label: run_label,
            task_type: "shell".to_string(),
            command: "cargo".to_string(),
            args: Some(vec!["run".to_string()]),
            cwd: cwd.clone(),
            group: Some(TaskGroup::Simple("build".to_string())),
            presentation: Some(serde_json::json!({
                "echo": true,
                "reveal": "always",
                "focus": true,
                "panel": "dedicated"
            })),
            problem_matcher: Some(vec!["$rustc".to_string()]),
            run_options: None,
        });
    }

    WorkspaceTasks {
        version: "2.0.0".to_string(),
        tasks,
    }
}

/// Write a workspace file for the given runnables and root directory.
/// Returns the path to the written workspace file on success.
///
/// Preservation and merge behavior:
/// - If an existing `.code-workspace` file is present, its `folders` entries are preserved and used as the
///   base instead of being overwritten.
/// - Discovered Cargo project folders are appended only when no existing folder resolves to the same
///   project path. This avoids adding duplicate folder entries when the user already added the project.
/// - Path comparisons attempt to canonicalize the discovered project path and each existing folder path
///   (resolving relative paths against `root_dir`) so that absolute and relative folder forms match.
/// - When an existing folder path matches a discovered project, the existing folder's `name` is reused
///   for generated launchers and tasks. This ensures `${workspaceFolder:<name>}` in generated
///   launches/tasks reference the user's preserved folder name.
/// - If that fails (for example the path does not exist yet), a best-effort resolved comparison
///   is used. We also back up the previous workspace file and attempt a simple cleanup if the
///   existing file is malformed.
pub fn write_workspace_for_root(output_dir: &Path, runnables: &[Runnable], root_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let workspace_filename = generate_workspace_filename(root_dir);
    let workspace_path = output_dir.join(&workspace_filename);

    let mut workspace_file: WorkspaceFile = if workspace_path.exists() {
        let base_backup_name = format!("{}.backup", workspace_filename);
        let mut backup_path = output_dir.join(&base_backup_name);
        if backup_path.exists() {
            let mut counter = 1;
            loop {
                backup_path = output_dir.join(format!("{}.{}", base_backup_name, counter));
                if !backup_path.exists() { break; }
                counter += 1;
            }
        }
        fs::copy(&workspace_path, &backup_path)?;

        let content = fs::read_to_string(&workspace_path)?;
        match serde_json::from_str::<WorkspaceFile>(&content) {
            Ok(w) => w,
            Err(parse_err) => {
                eprintln!("Warning: Failed to parse existing workspace file: {}. Attempting cleanup.", parse_err);
                let trailing_comma_re = Regex::new(r",(\s*[}\]])").unwrap();
                let cleaned = trailing_comma_re.replace_all(&content, "$1").to_string();
                match serde_json::from_str::<WorkspaceFile>(&cleaned) {
                    Ok(w2) => w2,
                    Err(_) => WorkspaceFile::default(),
                }
            }
        }
    } else {
        WorkspaceFile::default()
    };

    // Collect unique project paths
    let mut project_paths: Vec<PathBuf> = runnables.iter().map(|r| r.project_path.clone()).collect();
    project_paths.sort();
    project_paths.dedup();

    let workspace_name = generate_workspace_name(root_dir, &project_paths);
    workspace_file.name = Some(workspace_name);

    // Create the unique folder names
    let mut name_map: HashMap<PathBuf, String> = HashMap::new();
    let mut used_names: HashSet<String> = HashSet::new();
    for project_path in &project_paths {
        let mut candidate = project_path.file_name().and_then(|n| n.to_str()).unwrap_or("root").to_string();
        if candidate == "." || candidate.is_empty() { candidate = "root".to_string(); }
        candidate = candidate.replace(' ', "_");
        let original = candidate.clone();
        let mut suffix = 1;
        while used_names.contains(&candidate) {
            candidate = format!("{}-{}", original, suffix);
            suffix += 1;
        }
        used_names.insert(candidate.clone());
        name_map.insert(project_path.clone(), candidate);
    }

    // Preserve the user's existing folders and append any discovered projects that
    // aren't already listed.
    let mut folders: Vec<WorkspaceFolder> = if !workspace_file.folders.is_empty() {
        workspace_file.folders.clone() // Use the existing folder names as the base
    } else {
        Vec::new()
    };

    // We compare the folder path strings as they are. 
    // For the projects we discover, we build the same relative path
    // format and only add them if they don't already exist.
    for project_path in &project_paths {
        // Build the relative path string that exists in the workspace file (e.g. "path": "../planets_example_03"
        let relative_path = match pathdiff::diff_paths(&project_path, root_dir) {
            Some(path) if path != Path::new("") && path != Path::new(".") => format!("./{}", path.display()),
            _ => ".".to_string(),
        };

        // Try to clean up the project path so it matches the format of existing workspace folders.
        // If that doesn't work (like if the path doesn't exist yet), just use the original path instead.
        let absolute_project_path = project_path.canonicalize().unwrap_or(project_path.clone());

        // Look through the folders and make each path absolute using the root folder if needed.
        // If we can't clean it up properly, just use the version we already have.
        let mut already_present = false;
        for existing in &folders {
            let existing_path_str = &existing.path;
            let resolved_existing = if Path::new(existing_path_str).is_absolute() {
                PathBuf::from(existing_path_str).canonicalize().unwrap_or(PathBuf::from(existing_path_str))
            } else {
                root_dir.join(existing_path_str).canonicalize().unwrap_or(root_dir.join(existing_path_str))
            };

            if resolved_existing == absolute_project_path {
                // If the existing folder has a name, prefer that name for this project so
                // generated launches/tasks will reference the correct ${workspaceFolder:<name>}.
                if let Some(existing_name) = existing.name.clone() {
                    name_map.insert(project_path.clone(), existing_name);
                }
                already_present = true;
                break;
            }
        }

        if !already_present {
            let folder_name = name_map.get(project_path).cloned();
            folders.push(WorkspaceFolder { path: relative_path, name: folder_name });
        }
    }

    if folders.is_empty() {
        folders.push(WorkspaceFolder { path: ".".to_string(), name: Some("root".to_string()) });
    }

    workspace_file.folders = folders;

    if workspace_file.settings.as_ref().map_or(false, |s| s.is_null()) {
        workspace_file.settings = None;
    }
    if workspace_file.extensions.as_ref().map_or(false, |e| e.is_null() || (e.is_object() && e.as_object().unwrap().is_empty())) {
        workspace_file.extensions = None;
    }

    let launch = generate_launch_config(runnables, root_dir);
    workspace_file.launch = Some(WorkspaceLaunchConfig { version: launch.version, configurations: launch.configurations });

    let default_tasks = generate_default_tasks_internal(&project_paths, &name_map);
    workspace_file.tasks = Some(default_tasks);

        // If the OPEN_SPACE_MMO environment variable is set to "1", look for and merge the
        // ~/.config/open_space_mmo_workspace_settings.json settings into the workspace file. 
        // We add only keys that do not already exist in the user's workspace settings so we
        // don't overwrite existing preferences. This prevents overwriting open space mmo contributor preferences.
        // If OPEN_SPACE_MMO is not set, we look for any *workspace_settings.json file
        // in the workspace root and merge that instead.
        let use_home = std::env::var("OPEN_SPACE_MMO").map(|v| v == "1").unwrap_or(false);
        let mut selected_config_path: Option<PathBuf> = None;
        if use_home {
            if let Ok(home) = std::env::var("HOME") {
                let cfg = PathBuf::from(home).join(".config").join("open_space_mmo_workspace_settings.json");
                if cfg.exists() {
                    selected_config_path = Some(cfg);
                }
            }
        } else {
            // Look for a file in the workspace root that ends with `workspace_settings.json`.
            if let Ok(entries) = fs::read_dir(output_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                            if name.ends_with("workspace_settings.json") {
                                selected_config_path = Some(p);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if let Some(config_path) = selected_config_path {
            match fs::read_to_string(&config_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(mut selected_val) => {
                        // If the external config is a workspace file with a "settings" key,
                        // extract the settings object to merge
                        if selected_val.is_object() {
                            let obj = selected_val.as_object().unwrap();
                            if obj.contains_key("settings") && obj.get("settings").unwrap().is_object() {
                                selected_val = obj.get("settings").unwrap().clone();
                            }
                        }

                        if selected_val.is_object() {
                            if let Some(existing_settings) = workspace_file.settings.as_mut() {
                                if existing_settings.is_object() {
                                    let existing_map = existing_settings.as_object_mut().unwrap();
                                    for (k, v) in selected_val.as_object().unwrap().iter() {
                                        if !existing_map.contains_key(k) {
                                            existing_map.insert(k.clone(), v.clone());
                                        }
                                    }
                                } else {
                                    // Existing settings aren't an object; replace them with the external settings.
                                    workspace_file.settings = Some(selected_val);
                                }
                            } else {
                                workspace_file.settings = Some(selected_val);
                            }
                        }
                    }
                    Err(parse_err) => {
                        eprintln!("Error: failed to parse {}: {}", config_path.display(), parse_err);
                        // If stdin is not a TTY (non-interactive), abort instead of
                        // silently continuing.
                        if !atty::is(Stream::Stdin) {
                            return Err(format!("Aborted: {} is invalid JSON: {}", config_path.display(), parse_err).into());
                        }
                        // Prompt the user whether to continue without applying these settings.
                        eprint!("Continue generating workspace without these settings? (y/N): ");
                        use std::io::{self, Write};
                        io::stdout().flush().ok();
                        let mut input = String::new();
                        match io::stdin().read_line(&mut input) {
                            Ok(_) => {
                                let ans = input.trim();
                                if ans.eq_ignore_ascii_case("y") || ans.eq_ignore_ascii_case("yes") {
                                    eprintln!("Continuing without applying open_space_mmo settings.");
                                } else {
                                    return Err(format!("Aborted by user due to invalid {}: {}", config_path.display(), parse_err).into());
                                }
                            }
                            Err(_) => {
                                // In interactive mode a read_line failure is unexpected;
                                // abort to be safe.
                                return Err(format!("Aborted: failed to read user input while handling invalid {}: {}", config_path.display(), parse_err).into());
                            }
                        }
                    }
                },
                Err(read_err) => {
                    eprintln!("Error: failed to read {}: {}", config_path.display(), read_err);
                    if !atty::is(Stream::Stdin) {
                        return Err(format!("Aborted: could not read {}: {}", config_path.display(), read_err).into());
                    }
                    eprint!("Continue generating workspace without these settings? (y/N): ");
                    use std::io::{self, Write};
                    io::stdout().flush().ok();
                    let mut input = String::new();
                    match io::stdin().read_line(&mut input) {
                        Ok(_) => {
                            let ans = input.trim();
                            if ans.eq_ignore_ascii_case("y") || ans.eq_ignore_ascii_case("yes") {
                                eprintln!("Continuing without applying open_space_mmo settings.");
                            } else {
                                return Err(format!("Aborted by user due to unreadable {}: {}", config_path.display(), read_err).into());
                            }
                        }
                        Err(_) => {
                            return Err(format!("Aborted: failed to read user input while handling unreadable {}: {}", config_path.display(), read_err).into());
                        }
                    }
                }
            }
        }

    let json_content = serde_json::to_string_pretty(&workspace_file)?;
    fs::write(&workspace_path, json_content)?;

    Ok(workspace_path)
}
