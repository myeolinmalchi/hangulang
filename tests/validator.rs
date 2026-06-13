//! Reference-validator integration test.
//!
//! Converts every fixture in both modes and runs the output through the Python
//! DocLang reference validator (`doclang validate <file> -n`). Asserts the
//! validator exits 0 for every (fixture, mode).
//!
//! ## Why feature-gated
//!
//! The validator requires a Python venv with `doclang==0.6.0` installed, which
//! is not present in every environment. The test only runs under:
//!
//! ```text
//! python3.12 -m venv .venv
//! .venv/bin/pip install -r tests/requirements.txt
//! cargo test --features validator-integration --test validator
//! ```
//!
//! Without the `validator-integration` feature the test body compiles to a
//! no-op. Even with the feature, if the venv/CLI is missing the test
//! auto-skips with an `eprintln!` rather than failing — so CI without Python
//! degrades gracefully.
//!
//! ## Confirmed CLI interface
//!
//! `doclang validate <xml_file> [-n] [-q]`, where `-n`
//! (`--allow-empty-namespace`) auto-injects the DocLang namespace so that our
//! namespace-less output validates against the bundled XSD + Schematron.

mod common;

#[cfg(feature = "validator-integration")]
mod validator_integration {
    use super::common::{all_fixtures, read_fixture};
    use hangulang::{convert, ConvertOptions, Mode};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Locate the `doclang` executable inside the project-local venv.
    /// Returns `None` if it is not present (→ the test skips).
    fn doclang_bin() -> Option<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for candidate in [".venv/bin/doclang", ".venv/Scripts/doclang.exe"] {
            let p = root.join(candidate);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    fn mode_str(mode: Mode) -> &'static str {
        match mode {
            Mode::Lean => "lean",
            Mode::Preserve => "preserve",
        }
    }

    #[test]
    fn all_fixtures_pass_reference_validator() {
        let Some(doclang) = doclang_bin() else {
            eprintln!(
                "SKIP: .venv/bin/doclang not found. Install with:\n  \
                 python3.12 -m venv .venv && \
                 .venv/bin/pip install -r tests/requirements.txt"
            );
            return;
        };

        let tmp = std::env::temp_dir().join("hangulang-validator");
        std::fs::create_dir_all(&tmp).expect("create temp dir");

        let fixtures = all_fixtures();
        assert!(!fixtures.is_empty(), "no fixtures to validate");

        let mut failures: Vec<String> = Vec::new();

        for fx in &fixtures {
            let data = read_fixture(fx);

            for mode in [Mode::Lean, Mode::Preserve] {
                let opts = ConvertOptions { mode, ..ConvertOptions::default() };
                let xml = match convert(&data, &opts) {
                    Ok(o) => o.xml,
                    Err(e) => {
                        failures.push(format!("{} [{}]: convert failed: {e}", fx.name, mode_str(mode)));
                        continue;
                    }
                };

                let out_path = tmp.join(format!("{}.{}.dclg.xml", fx.name, mode_str(mode)));
                std::fs::write(&out_path, xml.as_bytes()).expect("write temp xml");

                let output = Command::new(&doclang)
                    .arg("validate")
                    .arg(&out_path)
                    .arg("-n")
                    .output()
                    .expect("spawn doclang");

                if !output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Capture only the diagnostic lines for a compact report.
                    let detail: String = stdout
                        .lines()
                        .chain(stderr.lines())
                        .filter(|l| l.contains("Line ") || l.contains("FAILED") || l.contains("Error"))
                        .take(6)
                        .collect::<Vec<_>>()
                        .join("\n    ");
                    failures.push(format!(
                        "{} [{}]: validator exit {:?}\n    {}",
                        fx.name,
                        mode_str(mode),
                        output.status.code(),
                        detail
                    ));
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{} (fixture, mode) outputs failed reference validation:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
