use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::lang::Lang;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".agentlens-cache",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    ".venv",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "site-packages",
    "target",
    "venv",
];

pub const MANIFEST_NAMES: &[&str] = &[
    "Cargo.toml",
    "Pipfile",
    "build.gradle",
    "go.mod",
    "package.json",
    "pom.xml",
    "pyproject.toml",
    "requirements.txt",
    "setup.cfg",
    "setup.py",
    "tox.ini",
];

pub fn is_skipped_dir(name: &str) -> bool {
    SKIP_DIRS.contains(&name) || (name.starts_with('.') && name.len() > 1)
}

pub fn source_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return if Lang::from_path(root).is_some() {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
    }
    let mut out: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            !is_skipped_dir(&entry.file_name().to_string_lossy())
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| Lang::from_path(path).is_some())
        .collect();
    out.sort();
    out
}

pub fn manifests(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = WalkDir::new(root)
        .max_depth(2)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            !is_skipped_dir(&entry.file_name().to_string_lossy())
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    MANIFEST_NAMES.contains(&name) || name.starts_with("requirements")
                })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

pub fn relative(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
