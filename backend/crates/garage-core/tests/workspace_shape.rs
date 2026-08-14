use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MAX_RUST_SOURCE_LINES: usize = 500;

// backend/crates/**/src files over 500 lines, and why each is allowed to be.
// Keeping this rule as data makes every exception and its justification visible.
const FILE_SIZE_EXCEPTIONS: &[(&str, &str)] = &[(
    "backend/crates/garage-core/src/schema/prefs.rs",
    "the schema table is one table; splitting it would hide drift",
)];

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn workspace_root() -> io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("garage-core must live at backend/crates/garage-core"))
}

#[expect(
    clippy::disallowed_methods,
    reason = "this integration guard must ask Cargo for the workspace graph it is validating"
)]
fn cargo_metadata(workspace: &Path) -> std::io::Result<Output> {
    Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(workspace)
        .output()
}

fn workspace_dependency_graph(
    metadata: &serde_json::Value,
) -> io::Result<BTreeMap<String, BTreeSet<String>>> {
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| io::Error::other("cargo metadata has no packages array"))?;
    let mut graph = BTreeMap::new();

    for package in packages {
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("cargo metadata package has no name"))?;
        let dependencies = package
            .get("dependencies")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                io::Error::other(format!(
                    "cargo metadata package {name} has no dependencies array"
                ))
            })?;
        let dependency_names = dependencies
            .iter()
            .filter_map(|dependency| dependency.get("name"))
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect();
        graph.insert(name.to_owned(), dependency_names);
    }

    Ok(graph)
}

fn reachable_workspace_dependencies(
    graph: &BTreeMap<String, BTreeSet<String>>,
    root: &str,
) -> BTreeSet<String> {
    let mut pending = VecDeque::from([root.to_owned()]);
    let mut reached = BTreeSet::new();

    while let Some(package) = pending.pop_front() {
        let Some(dependencies) = graph.get(&package) else {
            continue;
        };
        for dependency in dependencies {
            if graph.contains_key(dependency) && reached.insert(dependency.clone()) {
                pending.push_back(dependency.clone());
            }
        }
    }

    reached.remove(root);
    reached
}

#[test]
fn render_crate_cannot_reach_preferences_lock_or_process_execution() -> TestResult {
    // The render path must never be able to take PREFERENCES_LOCK. This is the
    // structural half of that invariant: garage-prefs owns the lock and garage-proc
    // owns process execution, so making both crates unreachable from garage-render
    // means render is physically unable to reach either capability, rather than
    // relying on call-site discipline. Traversing Cargo's workspace graph also catches
    // a forbidden transitive path introduced through another crate.
    let workspace = workspace_root()?;
    let output = cargo_metadata(&workspace)?;
    assert!(
        output.status.success(),
        "cargo metadata failed ({}):\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let graph = workspace_dependency_graph(&metadata)?;
    assert!(
        graph.contains_key("garage-render"),
        "garage-render is missing from cargo metadata"
    );

    let reached = reachable_workspace_dependencies(&graph, "garage-render");
    let forbidden: BTreeSet<String> = ["garage-prefs", "garage-proc"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let violations: Vec<_> = reached.intersection(&forbidden).cloned().collect();
    assert!(
        violations.is_empty(),
        "garage-render reaches {violations:?}; this reopens a path from the render context to \
         PREFERENCES_LOCK or process execution"
    );
    Ok(())
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_rust_sources(&path, sources)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            sources.push(path);
        }
    }
    Ok(())
}

#[test]
fn crate_source_files_stay_within_the_line_cap() -> TestResult {
    let workspace = workspace_root()?;
    let repository = workspace
        .parent()
        .ok_or_else(|| io::Error::other("the backend workspace must be inside the repository"))?;
    let crates = workspace.join("crates");
    let mut sources = Vec::new();

    for entry in fs::read_dir(&crates)? {
        let crate_directory = entry?.path();
        let source_directory = crate_directory.join("src");
        if source_directory.is_dir() {
            collect_rust_sources(&source_directory, &mut sources)?;
        }
    }
    sources.sort();

    let mut violations = Vec::new();
    for path in sources {
        let relative = path
            .strip_prefix(repository)
            .map_err(|error| io::Error::other(error.to_string()))?
            .to_string_lossy();
        if FILE_SIZE_EXCEPTIONS
            .iter()
            .any(|(exception, _reason)| relative == *exception)
        {
            continue;
        }
        let line_count = fs::read_to_string(&path)?.lines().count();
        if line_count > MAX_RUST_SOURCE_LINES {
            violations.push(format!("{relative}: {line_count} lines"));
        }
    }

    assert!(
        violations.is_empty(),
        "Rust source files exceed the {MAX_RUST_SOURCE_LINES}-line cap:\n{}",
        violations.join("\n")
    );
    Ok(())
}
