use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::calls::{self, CallSite, Import};
use crate::error::Result;
use crate::lang::Lang;
use crate::render::slash_path;
use crate::source::SourceFile;
use crate::symbols::{self, Symbol, SymbolKind};
use crate::walk;

pub const INDEX_VERSION: u32 = 1;
pub const CACHE_DIR: &str = ".agentlens-cache";
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSymbol {
    pub dotted: String,
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub span_start: usize,
    pub span_end: usize,
    pub def_start: usize,
    pub sig_end: usize,
    pub signature: String,
    pub decorators: Vec<String>,
    pub params: Vec<String>,
    pub required: usize,
    pub max_args: Option<usize>,
    pub type_names: Vec<String>,
}

impl IndexSymbol {
    pub fn accepts(&self, arity: usize) -> bool {
        if arity < self.required {
            return false;
        }
        self.max_args.is_none_or(|max| arity <= max)
    }

    pub fn arity_label(&self) -> String {
        match self.max_args {
            None => format!("{}+ args", self.required),
            Some(max) if max == self.required => format!("{max} args"),
            Some(max) => format!("{}-{max} args", self.required),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub size: u64,
    pub mtime_ns: u128,
    pub hash: String,
    pub lines: usize,
    pub lang: String,
    pub is_test: bool,
    pub is_package_init: bool,
    pub has_main_guard: bool,
    pub exports: Vec<String>,
    pub symbols: Vec<IndexSymbol>,
    pub calls: Vec<CallSite>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Clone, Copy)]
pub struct IndexStats {
    pub files: usize,
    pub reparsed: usize,
    pub from_cache: bool,
}

impl Index {
    /// Build the symbol and call index for `root`, reusing the on-disk cache
    /// when `use_cache` is set and the cached entries are still valid.
    ///
    /// # Errors
    ///
    /// Propagates [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`] from
    /// the files it indexes.
    pub fn build(root: &Path, use_cache: bool) -> Result<(Self, IndexStats)> {
        let paths = walk::source_files(root);
        let previous = if use_cache {
            load(root).unwrap_or_default()
        } else {
            Self::default()
        };
        let from_cache = !previous.files.is_empty();

        let entries: Vec<(String, FileEntry, bool)> = paths
            .par_iter()
            .filter_map(|path| {
                let key = slash_path(&walk::relative(root, path));
                let cached = previous.files.get(&key);
                build_entry(path, &key, cached).map(|(entry, reparsed)| (key, entry, reparsed))
            })
            .collect();

        let reparsed = entries.iter().filter(|(_, _, fresh)| *fresh).count();
        let mut files = BTreeMap::new();
        for (key, entry, _) in entries {
            files.insert(key, entry);
        }
        let index = Self {
            version: INDEX_VERSION,
            files,
        };
        let stats = IndexStats {
            files: index.files.len(),
            reparsed,
            from_cache,
        };
        if use_cache {
            let _ = store(root, &index);
        }
        Ok((index, stats))
    }

    pub fn symbols_named<'a>(&'a self, name: &str) -> Vec<(&'a str, &'a IndexSymbol)> {
        let mut out = Vec::new();
        for (path, entry) in &self.files {
            for symbol in &entry.symbols {
                if symbol.name == name {
                    out.push((path.as_str(), symbol));
                }
            }
        }
        out
    }

    pub fn lookup<'a>(&'a self, path: &str, dotted: &str) -> Vec<&'a IndexSymbol> {
        self.files.get(path).map_or_else(Vec::new, |entry| {
            entry
                .symbols
                .iter()
                .filter(|symbol| symbol.dotted == dotted)
                .collect()
        })
    }

    pub fn calls_to<'a>(&'a self, name: &str) -> Vec<(&'a str, &'a CallSite)> {
        let mut out = Vec::new();
        for (path, entry) in &self.files {
            for call in &entry.calls {
                if call.name == name {
                    out.push((path.as_str(), call));
                }
            }
        }
        out
    }

    pub fn entry(&self, path: &str) -> Option<&FileEntry> {
        self.files.get(path)
    }

    pub fn total_lines(&self) -> usize {
        self.files.values().map(|entry| entry.lines).sum()
    }
}

fn build_entry(path: &Path, key: &str, cached: Option<&FileEntry>) -> Option<(FileEntry, bool)> {
    let metadata = fs::metadata(path).ok()?;
    let size = metadata.len();
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());

    if let Some(entry) = cached
        && entry.size == size
        && entry.mtime_ns == mtime_ns
    {
        return Some((entry.clone(), false));
    }

    let bytes = fs::read(path).ok()?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    if let Some(entry) = cached
        && entry.hash == hash
    {
        let mut refreshed = entry.clone();
        refreshed.size = size;
        refreshed.mtime_ns = mtime_ns;
        return Some((refreshed, false));
    }

    let text = String::from_utf8(bytes).ok()?;
    let lang = Lang::from_path(path)?;
    let file = SourceFile::from_text(path, lang, text).ok()?;
    let tree = symbols::extract(&file);
    let flat = symbols::flatten(&tree);
    let symbol_entries: Vec<IndexSymbol> = flat
        .iter()
        .map(|symbol| to_index_symbol(&file, symbol))
        .collect();

    Some((
        FileEntry {
            size,
            mtime_ns,
            hash,
            lines: file.line_count(),
            lang: lang.name().to_string(),
            is_test: is_test_path(key),
            is_package_init: key.ends_with("__init__.py"),
            has_main_guard: file.text.contains("__name__ =="),
            exports: dunder_all(&file),
            symbols: symbol_entries,
            calls: calls::call_sites(&file, &tree),
            imports: calls::imports(&file),
        },
        true,
    ))
}

fn to_index_symbol(file: &SourceFile, symbol: &Symbol) -> IndexSymbol {
    IndexSymbol {
        dotted: symbol.dotted.clone(),
        name: symbol.name.clone(),
        kind: symbol.kind,
        start_line: symbol.start_line,
        end_line: symbol.end_line,
        span_start: symbol.span_start,
        span_end: symbol.span_end,
        def_start: symbol.def_start,
        sig_end: symbol.sig_end,
        signature: symbols::signature(file, symbol, false),
        decorators: symbol.decorators.clone(),
        params: symbol.params.clone(),
        required: symbol.required,
        max_args: symbol.max_args,
        type_names: symbol.type_names.clone(),
    }
}

pub fn is_test_path(key: &str) -> bool {
    let name = key.rsplit('/').next().unwrap_or(key);
    key.contains("/tests/")
        || key.starts_with("tests/")
        || key.contains("/test/")
        || name.starts_with("test_")
        || name.ends_with("_test.py")
        || name == "conftest.py"
}

fn dunder_all(file: &SourceFile) -> Vec<String> {
    let mut out = Vec::new();
    let Some(start) = file.text.find("__all__") else {
        return out;
    };
    let tail = &file.text[start..];
    let Some(open) = tail.find('[') else {
        return out;
    };
    let Some(close) = tail[open..].find(']') else {
        return out;
    };
    for piece in tail[open + 1..open + close].split(',') {
        let name = piece.trim().trim_matches(['"', '\'']).trim();
        if !name.is_empty() {
            out.push(name.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn cache_path(root: &Path) -> PathBuf {
    root.join(CACHE_DIR).join(INDEX_FILE)
}

fn load(root: &Path) -> Option<Index> {
    let text = fs::read_to_string(cache_path(root)).ok()?;
    let index: Index = serde_json::from_str(&text).ok()?;
    if index.version != INDEX_VERSION {
        return None;
    }
    Some(index)
}

fn store(root: &Path, index: &Index) -> std::io::Result<()> {
    let dir = root.join(CACHE_DIR);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(".gitignore"), "*\n")?;
    let text = serde_json::to_string(index)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    fs::write(dir.join(INDEX_FILE), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_test_paths() {
        assert!(is_test_path("tests/test_users.py"));
        assert!(is_test_path("src/pkg/test_thing.py"));
        assert!(is_test_path("src/pkg/thing_test.py"));
        assert!(!is_test_path("src/api/users.py"));
    }

    #[test]
    fn arity_acceptance_respects_varargs() {
        let symbol = IndexSymbol {
            dotted: "f".into(),
            name: "f".into(),
            kind: SymbolKind::Function,
            start_line: 1,
            end_line: 1,
            span_start: 0,
            span_end: 1,
            def_start: 0,
            sig_end: 1,
            signature: "def f(a, *rest):".into(),
            decorators: Vec::new(),
            params: vec!["a".into(), "*rest".into()],
            required: 1,
            max_args: None,
            type_names: Vec::new(),
        };
        assert!(!symbol.accepts(0));
        assert!(symbol.accepts(9));
        assert_eq!(symbol.arity_label(), "1+ args");
    }
}
