# Rust VS Code Workspace Configurator

A command-line tool that recursively discovers Rust projects (including Cargo workspaces) and creates VS Code multi-root workspace configurations (written into the specified root directory as `<root_directory_name>.code-workspace`) with launch configurations for all discovered binaries and examples.

## Important notes

- The tool searches recursively for Rust projects (directories containing `Cargo.toml` files) starting from the provided `--root` directory (or the current working directory if `--root` is not supplied).
- **Supports both individual Rust packages and Cargo workspaces**: If the root directory contains a `Cargo.toml`, it processes that project directly (including all workspace members if it's a workspace). If it doesn't contain a `Cargo.toml`, it scans subdirectories to find all Rust projects.
- Creates a multi-root VS Code workspace with separate folders for each discovered Rust project.
- **For Cargo workspaces**: Discovers and creates launch configurations for binaries and examples across all workspace members that reside under the workspace root.
- The workspace filename is based on the root directory name (e.g., `my-project.code-workspace` for a directory named `my-project`).
- If a workspace file already exists at the output location, the tool makes a backup with `.backup` appended (e.g., `my-project.code-workspace.backup`). If that name is already taken, it will append `.1`, `.2`, etc. until an unused name is found.
 - When updating an existing `.code-workspace` the tool preserves any user-provided `folders` entries instead of overwriting them. Discovered Rust project folders are appended only when they don't already resolve to the same filesystem path (the tool attempts to canonicalize and resolve relative paths against the workspace root). When an existing folder matches a discovered project, the tool reuses the existing folder's `name` for generated `${workspaceFolder:<name>}` tokens so launches and tasks reference the preserved folder name.
- Generated launch configurations target the `lldb` debugger and assume you have an LLDB adapter in VS Code (e.g., the CodeLLDB extension).
- The generated launch configurations set an environment variable `BEVY_ASSET_ROOT` to the appropriate project directory. This is included for Rust projects using Bevy or similar frameworks; you can remove or modify this value in the generated workspace file if it's not applicable.
- If a target (binary or example) declares required Cargo features, the tool adds `--features` followed by a comma-separated features string (e.g., `"--features", "feat1,feat2"`) to the cargo invocation in the launch configuration.
- Launch configuration names follow the pattern `<target-name> (Bin)` for binaries and `<target-name> (Example)` for examples. When a package manifest differs from the project manifest, the generated `cargo.args` will include an explicit `--package <name>` entry to ensure the correct package is built and executed.
- The tool also generates VS Code tasks for building and running each discovered project.

## Features

- Recursively discovers all Rust projects (directories containing `Cargo.toml` files) in the specified directory tree.
- **Full Cargo workspace support**: Automatically detects workspace manifests and processes all workspace members that reside under the workspace root to discover their binaries and examples.
- Discovers `bin` targets and `example` targets for each found project/package using `cargo_metadata` with all features enabled.
- Generates LLDB-compatible debug configurations that run `cargo run` for each target with appropriate project-relative paths.
- Creates multi-root VS Code workspaces with proper folder structure for all discovered projects.
- Generates VS Code tasks for building and running each discovered project.
- Updates existing `<name>.code-workspace` files or creates new ones, preserving existing files by creating backups.
- Handles malformed existing workspace files gracefully by attempting to fix common JSON issues (e.g., trailing commas), creating backups, and starting fresh if parsing fails.

## Installation

Build and run from the repository or install with Cargo:

```bash
# build locally (debug build)
cargo build

# or build a release binary
cargo build --release
```

```bash
# execute the tool from the repository (no special flags required)
# use `--root <PATH>` or the short form `-r <PATH>` to point at a package folder
cargo run -- --root /path/to/rust/project

# run from inside a package directory (uses current directory)
cargo run --
```

```bash
# or install to your cargo bin
cargo install --path .
```

## Usage

Run the tool pointing at a directory containing Rust projects (defaults to current directory). You can use the long `--root <PATH>` or the short `-r <PATH>` flag.

```bash
# specifying a root path (long form) - searches for all Rust projects in this directory
rust-vscode-workspace-configurator --root /path/to/directory/containing/rust/projects

# specifying a root path (short form)
rust-vscode-workspace-configurator -r /path/to/directory/containing/rust/projects

# or from inside a directory (uses current directory)
rust-vscode-workspace-configurator
```

The tool will:

1. Check if the specified root directory contains a `Cargo.toml`.
   - **If it's a workspace manifest**: Processes all workspace members that reside under the workspace root to discover their binaries and examples.
   - **If it's a package manifest**: Processes that package directly.
2. If no `Cargo.toml` is found in the root, it recursively searches subdirectories for Rust projects (directories containing `Cargo.toml` files), skipping common non-project directories like `.git`, `target`, and `node_modules`.
3. Use `cargo metadata` (with all features enabled) to discover `bin` and `example` targets for each found project/package.
4. Generate launch configurations compatible with VS Code that invoke `cargo run`.

   The tool emits cargo arguments as separate tokens in the `cargo.args` array (e.g., `"--bin", "my_binary"` or `"--example", "example1"`). When a package manifest differs from the project manifest, the tool includes an explicit `--package` token (e.g., `"--package", "my_pkg"`) to disambiguate. The generated configurations also include `--target-dir` and `--manifest-path` tokens so cargo uses the correct build output directory and manifest file. If a target declares required features, the tool adds `"--features", "feat1,feat2"` as two tokens (flag and comma-separated feature string).
5. Create a multi-root workspace configuration with separate folders for each discovered project.
6. Generate VS Code tasks for building and running each discovered project.
7. Generate a workspace filename based on the root directory name (e.g., `my-projects.code-workspace`).
8. Write or update the workspace file in the specified root, creating a backup of any existing file with `.backup` appended and adding numeric suffixes (`.1`, `.2`, ...) if needed.

## Example output

When run in a directory containing multiple Rust projects, you might see:

```text
Searching for Rust projects in: /path/to/projects
Found 3 Rust project(s):
  /path/to/projects/project1
  /path/to/projects/project2
  /path/to/projects/project3
Found 5 runnables:
  project1::my_binary (Binary) in package project1
  project1::example1 (example) (Example) in package project1
  project2::server (Binary) in package project2
  project3::cli_tool (Binary) in package project3
  project3::integration_test (example) (Example) in package project3
Backed up existing projects.code-workspace to projects.code-workspace.backup.1
Created projects.code-workspace with launch configurations in /path/to/projects
```

When run on a **Cargo workspace**, you might see output like this:

```text
Searching for Rust projects in: /path/to/my-workspace
Found 1 Rust project(s):
  /path/to/my-workspace
Found 18 runnables:
  bevy-text3d::basic (example) (Example) in package bevy-text3d
  bevy-text3d::custom_color (example) (Example) in package bevy-text3d
  bevy_renet_test::client (example) (Example) in package bevy_renet_test
  bevy_renet_test::server (example) (Example) in package bevy_renet_test
  fullscreen_toggle::fullscreen_toggle (Binary) in package fullscreen_toggle
  gabriels-horn::gabriels-horn (Binary) in package gabriels-horn
  render-debug::grid_lines (example) (Example) in package render-debug
  # ... more binaries and examples from workspace members
Backed up existing my-workspace.code-workspace to my-workspace.code-workspace.backup
Created my-workspace.code-workspace with launch configurations in /path/to/my-workspace
```

And `projects.code-workspace` will contain a multi-root workspace configuration with a top-level `launch` object similar to the following.

Here is a short, pretty-printed example `projects.code-workspace` showing a multi-root workspace with three projects and their launch configurations. Note how each project gets its own folder and launch configurations are namespaced:

```json
{
  "folders": [
    { "path": "./project1" },
    { "path": "./project2" },
    { "path": "./project3" }
  ],
  "launch": {
    "version": "0.2.0",
    "configurations": [
      {
        "name": "my_binary (Bin)",
        "type": "lldb",
        "request": "launch",
        "cwd": "${workspaceFolder}/project1",
        "env": { "BEVY_ASSET_ROOT": "${workspaceFolder}/project1" },
        "cargo": {
          "args": [
            "run",
            "--bin",
            "my_binary",
            "--package",
            "project1",
            "--features",
            "cool-feature",
            "--target-dir",
            "${workspaceFolder}/project1/target",
            "--manifest-path",
            "${workspaceFolder}/project1/Cargo.toml"
          ],
          "cwd": "${workspaceFolder}/project1"
        },
        "args": []
      },
      {
        "name": "example1 (Example)",
        "type": "lldb",
        "request": "launch",
        "cwd": "${workspaceFolder}/project1",
        "env": { "BEVY_ASSET_ROOT": "${workspaceFolder}/project1" },
        "cargo": {
          "args": [
            "run",
            "--example",
            "example1",
            "--package",
            "project1",
            "--target-dir",
            "${workspaceFolder}/project1/target",
            "--manifest-path",
            "${workspaceFolder}/project1/Cargo.toml"
          ],
          "cwd": "${workspaceFolder}/project1"
        },
        "args": []
      },
      {
        "name": "server (Bin)",
        "type": "lldb",
        "request": "launch",
        "cwd": "${workspaceFolder}/project2",
        "env": { "BEVY_ASSET_ROOT": "${workspaceFolder}/project2" },
        "cargo": {
          "args": [
            "run",
            "--bin",
            "server",
            "--package",
            "project2",
            "--target-dir",
            "${workspaceFolder}/project2/target",
            "--manifest-path",
            "${workspaceFolder}/project2/Cargo.toml"
          ],
          "cwd": "${workspaceFolder}/project2"
        },
        "args": []
      }
    ]
  }
}
```

Notes:

- Generated configurations are named `<target> (Bin)` for binaries and `<target> (Example)` for examples (e.g., `my_binary (Bin)` or `example1 (Example)`).
- Each configuration sets `type` to `lldb`, `request` to `launch`, and `cwd` to the appropriate project directory relative to the workspace folder.
- The `env.BEVY_ASSET_ROOT` is set to the project directory to support projects using Bevy or similar frameworks that need an asset root.
- The `cargo.args` array contains the `cargo run` subcommand and a tokenized list of flags and values; `--features` is added when targets declare required features as two tokens (`"--features", "feat1,feat2"`). When a package manifest differs from the project manifest, the tool includes an explicit `--package` token to ensure the correct package is built.
- Multi-root workspaces allow you to work with multiple Rust projects simultaneously while maintaining proper project isolation.

## VS Code Workspace Identification

The tool generates workspace files with descriptive names based on the root directory name (e.g., `my-projects.code-workspace`). This naming convention helps VS Code differentiate between workspaces in several ways:

- **Recent Workspaces List**: VS Code displays workspace files by their filename, making it easy to identify which workspace contains which projects.
- **Window Titles**: VS Code uses the workspace filename in window titles, helping you distinguish between multiple VS Code instances.
- **Workspace Settings**: Each workspace maintains its own settings, extensions, and state based on the workspace file path.
- **Global State**: VS Code tracks workspace-specific data (like recently opened files, search history, etc.) using the workspace file path as a unique identifier.

The generated workspace files also include a `name` property set to descriptive values like:

- `<project-name> (Rust)` for single-project workspaces
- `<root-directory> (N Rust Projects)` for multi-project workspaces

This provides a user-friendly name in VS Code's workspace switcher and other UI elements.

## Dependencies

- `serde` and `serde_json` for JSON serialization/deserialization
- `clap` for command-line argument parsing
- `cargo_metadata` for reading Cargo metadata
- `pathdiff` for calculating relative paths between directories
- `regex` for cleaning malformed JSON in existing workspace files

## License

MIT or Apache-2.0 (your choice)
