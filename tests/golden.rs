//! Golden-file regression tests.
//!
//! For every fixture under `tests/fixtures/` (excluding `golden/`), this test
//! converts the document in BOTH `lean` and `preserve` modes and compares the
//! emitted DocLang XML byte-for-byte against a committed baseline at
//! `tests/fixtures/golden/<name>.<mode>.dclg.xml`.
//!
//! ## Regenerating the baseline
//!
//! When the converter's output legitimately changes, regenerate the goldens and
//! review the diff before committing:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test --test golden
//! git diff tests/fixtures/golden
//! ```
//!
//! Conversion is deterministic (verified: repeated runs are byte-identical),
//! so a committed golden is a stable contract.

mod common;

use std::fs;
use std::path::PathBuf;

use hangulang::{convert, ConvertOptions, Mode};

use common::{all_fixtures, read_fixture};

fn golden_dir() -> PathBuf {
    common::fixtures_dir().join("golden")
}

fn golden_path(name: &str, mode: Mode) -> PathBuf {
    let mode_str = match mode {
        Mode::Lean => "lean",
        Mode::Preserve => "preserve",
    };
    golden_dir().join(format!("{name}.{mode_str}.dclg.xml"))
}

fn update_golden() -> bool {
    std::env::var("UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false)
}

#[test]
fn golden_corpus_matches() {
    let fixtures = all_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under tests/fixtures — corpus is missing"
    );

    if update_golden() {
        fs::create_dir_all(golden_dir()).expect("create golden dir");
    }

    let mut mismatches: Vec<String> = Vec::new();

    for fx in &fixtures {
        let data = read_fixture(fx);

        for mode in [Mode::Lean, Mode::Preserve] {
            let opts = ConvertOptions { mode, ..ConvertOptions::default() };
            let outcome = match convert(&data, &opts) {
                Ok(o) => o,
                Err(e) => {
                    mismatches.push(format!(
                        "{} [{:?}]: conversion failed: {e}",
                        fx.name, mode
                    ));
                    continue;
                }
            };

            let path = golden_path(&fx.name, mode);

            if update_golden() {
                fs::write(&path, outcome.xml.as_bytes())
                    .unwrap_or_else(|e| panic!("write golden {}: {e}", path.display()));
                continue;
            }

            let expected = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => {
                    mismatches.push(format!(
                        "{} [{:?}]: missing golden {} (run UPDATE_GOLDEN=1)",
                        fx.name,
                        mode,
                        path.display()
                    ));
                    continue;
                }
            };

            if expected != outcome.xml {
                mismatches.push(format!(
                    "{} [{:?}]: output differs from golden {} \
                     (expected {} bytes, got {} bytes)",
                    fx.name,
                    mode,
                    path.display(),
                    expected.len(),
                    outcome.xml.len()
                ));
            }
        }
    }

    if update_golden() {
        eprintln!(
            "UPDATE_GOLDEN=1: regenerated {} golden files ({} fixtures x 2 modes)",
            fixtures.len() * 2,
            fixtures.len()
        );
        return;
    }

    assert!(
        mismatches.is_empty(),
        "golden mismatches ({}):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// Sanity checks on the generated output that don't depend on exact bytes:
/// every fixture must produce a well-rooted DocLang document with the right
/// version, and at least one fixture must carry real (Korean) text content.
#[test]
fn output_is_structurally_sane() {
    let fixtures = all_fixtures();
    let mut saw_korean = false;

    for fx in &fixtures {
        let data = read_fixture(fx);
        let outcome = convert(&data, &ConvertOptions::default())
            .unwrap_or_else(|e| panic!("{}: convert failed: {e}", fx.name));

        let xml = &outcome.xml;
        assert!(
            xml.starts_with("<doclang version=\"0.6\">"),
            "{}: missing/incorrect doclang root: {:?}",
            fx.name,
            &xml[..xml.len().min(40)]
        );
        assert!(
            xml.ends_with("</doclang>"),
            "{}: document is not closed",
            fx.name
        );
        assert!(!xml.is_empty(), "{}: empty output", fx.name);

        // Detect any Hangul syllable — proves text decoding actually worked.
        if xml.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)) {
            saw_korean = true;
        }
    }

    assert!(
        saw_korean,
        "no fixture produced any Korean text — decoding is likely broken"
    );
}
