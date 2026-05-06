//! Resolves clipboard text into something shareable.
//!
//! When a user hits the share hotkey, the clipboard could contain anything:
//! an absolute path, a relative path that came from VS Code's "Copy Relative
//! Path", a `file://` URL, an `http(s)://` URL, or junk. This module
//! classifies the input and, for relative paths, searches a configurable
//! list of project roots so the agent (and the human) doesn't have to worry
//! about CWD.
//!
//! Walking is done with `ignore` so .gitignore is respected — node_modules,
//! target/, etc. are skipped automatically.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// One filesystem hit, with enough metadata to disambiguate in the picker.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub path: String,
    pub parent_dir: String,
    pub size_bytes: u64,
    pub mtime_secs: u64,
    /// "file" or "dir" — the modal can show different copy for folder shares.
    pub kind: CandidateKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CandidateKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Resolution {
    /// Empty clipboard.
    Empty,
    /// Clipboard had something we couldn't interpret as a file.
    Unsupported { raw: String },
    /// http(s) URL — caller can fetch.
    Url { url: String },
    /// Single unambiguous local path.
    Path { candidate: Candidate },
    /// Multiple matches; user picks.
    Candidates { candidates: Vec<Candidate> },
    /// Searched and found nothing.
    NotFound { query: String, roots: Vec<String> },
}

/// Default project roots probed when a relative or basename-only string is
/// pasted. Settings UI (step 16) lets users edit this list.
pub fn default_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for sub in [
            "Github",
            "github",
            "Projects",
            "projects",
            "Code",
            "code",
            "src",
            "dev",
            "Developer",
            "Documents",
            "Desktop",
        ] {
            let p = home.join(sub);
            if p.exists() {
                out.push(p);
            }
        }
    }
    out
}

/// Classify and (if needed) search.
pub fn resolve(raw: &str, roots: &[PathBuf]) -> Resolution {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Resolution::Empty;
    }

    // file:// URL — strip and treat as path.
    let stripped = trimmed.strip_prefix("file://").map(str::to_string);

    // http(s) URL.
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Resolution::Url {
            url: trimmed.to_string(),
        };
    }

    let as_path = stripped.as_deref().unwrap_or(trimmed);

    // Absolute or home-relative? Try direct stat first.
    if let Some(p) = expand_direct(as_path) {
        if p.exists() {
            return single(&p);
        }
    }

    // Note: deliberately NOT falling back to CWD here. The daemon's CWD is
    // implementation detail (wherever the user launched the binary from)
    // and doesn't reflect intent for clipboard-driven flows. Resolving via
    // CWD silently shadows root search and surprises users who expect a
    // basename to find matches across all their projects. CLI callers that
    // need CWD resolution should expand the path before calling.

    // Walk roots, collect suffix matches.
    let mut hits = search(as_path, roots, /* hard_limit */ 50);
    hits.sort_by_key(|c| std::cmp::Reverse(c.mtime_secs));
    hits.truncate(5);

    if hits.is_empty() {
        if !looks_like_path(as_path) {
            return Resolution::Unsupported {
                raw: trimmed.to_string(),
            };
        }
        return Resolution::NotFound {
            query: as_path.to_string(),
            roots: roots.iter().map(|r| r.display().to_string()).collect(),
        };
    }

    if hits.len() == 1 {
        return Resolution::Path {
            candidate: hits.remove(0),
        };
    }
    Resolution::Candidates { candidates: hits }
}

fn expand_direct(s: &str) -> Option<PathBuf> {
    if let Some(rest) = s.strip_prefix("~/") {
        dirs::home_dir().map(|h| h.join(rest))
    } else if s == "~" {
        dirs::home_dir()
    } else if s.starts_with('/') {
        Some(PathBuf::from(s))
    } else {
        None
    }
}

/// True iff the string looks like a file path, vs. e.g. a sentence.
fn looks_like_path(s: &str) -> bool {
    if s.contains('\n') || s.len() > 1024 {
        return false;
    }
    if s.contains('/') || s.contains('.') {
        return true;
    }
    false
}

fn search(query: &str, roots: &[PathBuf], hard_limit: usize) -> Vec<Candidate> {
    let mut out = Vec::new();
    let basename_only = !query.contains('/');

    for root in roots {
        if !root.exists() {
            continue;
        }
        // add_custom_ignore_filename forces .gitignore parsing even outside
        // git repos. Without it, a .gitignore in e.g. ~/Code/notes (not a
        // git repo) would be ignored and we'd walk into node_modules.
        let mut wb = ignore::WalkBuilder::new(root);
        wb.max_depth(Some(10))
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .add_custom_ignore_filename(".gitignore");
        let walker = wb.build();
        for entry in walker {
            if out.len() >= hard_limit {
                break;
            }
            let Ok(entry) = entry else { continue };
            let Some(ft) = entry.file_type() else {
                continue;
            };
            // Search only finds files. Direct path resolution (single()) is
            // what handles directories — that path doesn't go through this loop.
            if !ft.is_file() {
                continue;
            }
            let path = entry.path();
            if !matches_query(path, query, basename_only) {
                continue;
            }
            if let Some(c) = candidate_from(path) {
                out.push(c);
            }
        }
    }
    out
}

fn matches_query(path: &Path, query: &str, basename_only: bool) -> bool {
    let s = path.to_string_lossy();
    if basename_only {
        return path
            .file_name()
            .map(|n| n.to_string_lossy() == query)
            .unwrap_or(false);
    }
    // Suffix match: does the path end with /<query> or equal query?
    s.ends_with(query) && {
        let prefix_len = s.len().saturating_sub(query.len());
        prefix_len == 0 || s.as_bytes().get(prefix_len.saturating_sub(1)) == Some(&b'/')
    }
}

fn candidate_from(path: &Path) -> Option<Candidate> {
    let meta = std::fs::metadata(path).ok()?;
    let kind = if meta.is_dir() {
        CandidateKind::Dir
    } else if meta.is_file() {
        CandidateKind::File
    } else {
        return None;
    };
    let mtime_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(Candidate {
        path: path.display().to_string(),
        parent_dir: path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        size_bytes: meta.len(),
        mtime_secs,
        kind,
    })
}

fn single(path: &Path) -> Resolution {
    match candidate_from(path) {
        Some(c) => Resolution::Path { candidate: c },
        None => Resolution::NotFound {
            query: path.display().to_string(),
            roots: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn empty_returns_empty() {
        assert!(matches!(resolve("", &[]), Resolution::Empty));
        assert!(matches!(resolve("   \n  ", &[]), Resolution::Empty));
    }

    #[test]
    fn http_url_returns_url() {
        match resolve("https://example.com/foo.md", &[]) {
            Resolution::Url { url } => assert_eq!(url, "https://example.com/foo.md"),
            other => panic!("expected Url, got {other:?}"),
        }
        match resolve("http://example.com/", &[]) {
            Resolution::Url { .. } => (),
            other => panic!("expected Url, got {other:?}"),
        }
    }

    #[test]
    fn absolute_existing_path_returns_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hello.md");
        fs::write(&target, b"# hi").unwrap();
        match resolve(target.to_str().unwrap(), &[]) {
            Resolution::Path { candidate } => {
                assert_eq!(candidate.path, target.display().to_string());
                assert_eq!(candidate.size_bytes, 4);
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn file_url_strips_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.md");
        fs::write(&target, b"x").unwrap();
        let url = format!("file://{}", target.display());
        match resolve(&url, &[]) {
            Resolution::Path { candidate } => assert_eq!(candidate.path, target.display().to_string()),
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn relative_path_searches_roots() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("src/lib/foo.ts");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, b"x").unwrap();

        // Relative-with-prefix match
        match resolve("src/lib/foo.ts", std::slice::from_ref(&root.path().to_path_buf())) {
            Resolution::Path { candidate } => {
                assert_eq!(candidate.path, nested.display().to_string());
            }
            other => panic!("expected Path, got {other:?}"),
        }

        // Just a basename — also resolves when only one match
        match resolve("foo.ts", std::slice::from_ref(&root.path().to_path_buf())) {
            Resolution::Path { candidate } => {
                assert_eq!(candidate.path, nested.display().to_string());
            }
            other => panic!("expected Path, got {other:?}"),
        }
    }

    #[test]
    fn ambiguous_basename_returns_candidates() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("dir1/conflict.md");
        let b = root.path().join("dir2/conflict.md");
        fs::create_dir_all(a.parent().unwrap()).unwrap();
        fs::create_dir_all(b.parent().unwrap()).unwrap();
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();

        match resolve("conflict.md", std::slice::from_ref(&root.path().to_path_buf())) {
            Resolution::Candidates { candidates } => {
                assert_eq!(candidates.len(), 2);
                let paths: Vec<_> = candidates.iter().map(|c| c.path.clone()).collect();
                assert!(paths.contains(&a.display().to_string()));
                assert!(paths.contains(&b.display().to_string()));
            }
            other => panic!("expected Candidates, got {other:?}"),
        }
    }

    #[test]
    fn missing_relative_returns_not_found_when_path_shaped() {
        let root = tempfile::tempdir().unwrap();
        match resolve("src/some/missing.ts", std::slice::from_ref(&root.path().to_path_buf())) {
            Resolution::NotFound { query, .. } => assert_eq!(query, "src/some/missing.ts"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn random_text_returns_unsupported() {
        match resolve("just a sentence here", &[]) {
            Resolution::Unsupported { .. } => (),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn respects_gitignore() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), "node_modules\n").unwrap();
        let hidden = root.path().join("node_modules/foo.md");
        let visible = root.path().join("src/foo.md");
        fs::create_dir_all(hidden.parent().unwrap()).unwrap();
        fs::create_dir_all(visible.parent().unwrap()).unwrap();
        fs::write(&hidden, b"x").unwrap();
        fs::write(&visible, b"x").unwrap();

        match resolve("foo.md", std::slice::from_ref(&root.path().to_path_buf())) {
            Resolution::Path { candidate } => {
                assert_eq!(candidate.path, visible.display().to_string(),
                    "ignore crate should skip node_modules via .gitignore");
            }
            other => panic!("expected single Path (ignore should skip node_modules), got {other:?}"),
        }
    }
}
