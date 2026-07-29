use std::path::PathBuf;
use std::process::Command;

pub struct Case {
    pub name: &'static str,
    pub args: &'static [&'static str],
}

pub const fn case(name: &'static str, args: &'static [&'static str]) -> Case {
    Case { name, args }
}

pub fn fixture_repo() -> PathBuf {
    workspace_root().join("tests/fixtures/repo")
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

pub fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

pub fn run(args: &[&str]) -> String {
    run_in(&fixture_repo(), args)
}

pub fn run_in(dir: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_agentlens"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("agentlens runs");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut rendered = format!("$ agentlens {}\nexit: {code}\n", args.join(" "));
    rendered.push_str("--- stdout ---\n");
    rendered.push_str(&stdout);
    if !stdout.ends_with('\n') && !stdout.is_empty() {
        rendered.push('\n');
    }
    rendered.push_str("--- stderr ---\n");
    rendered.push_str(&stderr);
    if !stderr.ends_with('\n') && !stderr.is_empty() {
        rendered.push('\n');
    }
    rendered
}

pub fn check_snapshot(name: &str, actual: &str) {
    let dir = snapshot_dir();
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    let path = dir.join(format!("{name}.txt"));
    if std::env::var("AGENTLENS_UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(&path, actual).expect("write snapshot");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing snapshot {}\nrerun with AGENTLENS_UPDATE_SNAPSHOTS=1\n--- actual ---\n{actual}",
            path.display()
        )
    });
    assert_eq!(
        expected, actual,
        "snapshot mismatch for {name}: rerun with AGENTLENS_UPDATE_SNAPSHOTS=1 to accept"
    );
}
