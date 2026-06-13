//! Cross-format equivalence and lean-mode completeness tests.
//!
//! 1. **HWP5 ↔ HWPX equivalence** — the same source document saved in both the
//!    binary HWP5 (CFB) format and the zipped-XML HWPX format must convert to
//!    equivalent DocLang output. The pairs under `tests/fixtures/pairs/` are
//!    genuine same-content pairs; the converter produces byte-identical output
//!    for them, so after whitespace normalisation we assert equality.
//!
//! 2. **Lean-mode completeness** — a richly-formatted fixture must (a) report a
//!    non-empty `LossReport` in lean mode and (b) emit zero non-standard
//!    elements (no `<custom` / `<hwp_` substrings).

mod common;

use hangulang::{convert, ConvertOptions, Mode};

use common::fixtures_dir;

/// Collapse runs of ASCII whitespace to a single space and trim, so that
/// equality is robust to incidental formatting differences. (In practice the
/// two formats already produce identical bytes, but normalisation makes the
/// assertion's intent explicit and resilient.)
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn convert_file(rel: &str, mode: Mode) -> String {
    let path = fixtures_dir().join(rel);
    let data = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let opts = ConvertOptions { mode, ..ConvertOptions::default() };
    convert(&data, &opts)
        .unwrap_or_else(|e| panic!("convert {}: {e}", path.display()))
        .xml
}

/// The same document in HWP5 and HWPX must convert to equal output.
///
/// `pairs/para-001.{hwp,hwpx}` and `pairs/test-image.{hwp,hwpx}` are real
/// same-content pairs obtained from the upstream rhwp corpus (see SOURCES.md).
#[test]
fn hwp5_hwpx_pairs_convert_equivalently() {
    let pairs = [
        ("pairs/para-001.hwp", "pairs/para-001.hwpx"),
        ("pairs/test-image.hwp", "pairs/test-image.hwpx"),
    ];

    for (hwp, hwpx) in pairs {
        for mode in [Mode::Lean, Mode::Preserve] {
            let a = normalize_ws(&convert_file(hwp, mode));
            let b = normalize_ws(&convert_file(hwpx, mode));
            assert_eq!(
                a, b,
                "HWP5/HWPX output differs for ({hwp}, {hwpx}) in {mode:?} mode"
            );
        }
    }
}

/// Lean mode must report losses for a richly-formatted document and must never
/// emit non-standard elements.
#[test]
fn lean_mode_reports_loss_and_emits_no_custom() {
    // `tables/table-complex.hwp` carries fonts, sizes, colours, floating shapes
    // and merged cells — a rich source of lean-mode losses.
    let path = fixtures_dir().join("tables/table-complex.hwp");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let opts = ConvertOptions { mode: Mode::Lean, ..ConvertOptions::default() };
    let outcome = convert(&data, &opts).expect("convert table-complex (lean)");

    assert!(
        !outcome.loss.is_empty(),
        "lean mode produced an empty LossReport for a richly-formatted document"
    );

    // Lean output must be free of any non-standard / preserve-only markup.
    assert!(
        !outcome.xml.contains("<custom"),
        "lean output must not contain <custom> elements"
    );
    assert!(
        !outcome.xml.contains("<hwp_"),
        "lean output must not contain hwp_-prefixed elements"
    );
}

/// Companion to the above: in PRESERVE mode the same document SHOULD carry the
/// preserved properties as `<custom>` markup, confirming the two modes diverge
/// exactly where intended.
#[test]
fn preserve_mode_emits_custom_for_same_fixture() {
    let path = fixtures_dir().join("tables/table-complex.hwp");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let opts = ConvertOptions { mode: Mode::Preserve, ..ConvertOptions::default() };
    let outcome = convert(&data, &opts).expect("convert table-complex (preserve)");

    assert!(
        outcome.xml.contains("<custom>"),
        "preserve mode should emit <custom> markup for unmappable properties"
    );
}
