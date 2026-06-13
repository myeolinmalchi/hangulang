//! Shared helpers for the integration test suite.
//!
//! `tests/common/mod.rs` is a conventional Cargo location for code shared
//! between integration test binaries without itself being treated as a test
//! crate.
//!
//! Each test binary (`golden`, `validator`, `equivalence`) pulls in this module
//! but uses a different subset of it — and `validator` only uses it under the
//! `validator-integration` feature. `dead_code` is therefore allowed so the
//! unused-in-this-binary items don't generate warnings.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Absolute path to the `tests/fixtures` directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A discovered fixture: its logical name and absolute path.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Stable, filesystem-safe name, e.g. `tables__table-001`.
    pub name: String,
    /// Absolute path to the `.hwp` / `.hwpx` file.
    pub path: PathBuf,
}

/// Recursively collect every `.hwp` / `.hwpx` fixture under `tests/fixtures`,
/// excluding the `golden/` output directory.  Returns them sorted by name so
/// that golden generation and comparison are deterministic across platforms.
pub fn all_fixtures() -> Vec<Fixture> {
    let root = fixtures_dir();
    let mut out = Vec::new();
    collect(&root, &root, &mut out);
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Fixture>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip the generated golden corpus.
            if path.file_name().and_then(|n| n.to_str()) == Some("golden") {
                continue;
            }
            collect(root, &path, out);
            continue;
        }
        let is_hwp = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("hwp") | Some("hwpx")
        );
        if !is_hwp {
            continue;
        }
        // Build a stable name from the path relative to `fixtures/`,
        // replacing separators and the extension so it is filesystem-safe.
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let mut name = rel.with_extension("").to_string_lossy().replace(['/', '\\'], "__");
        // HWP5 and HWPX with the same stem (e.g. equivalence pairs) would
        // collide after extension stripping; disambiguate by extension.
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            name.push_str("__");
            name.push_str(ext);
        }
        out.push(Fixture { name, path });
    }
}

/// Read a fixture's bytes, panicking with a clear message on failure.
pub fn read_fixture(fx: &Fixture) -> Vec<u8> {
    std::fs::read(&fx.path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", fx.path.display()))
}
