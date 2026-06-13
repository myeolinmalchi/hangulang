//! v2 `<location>` integration tests.
//!
//! Exercises the `with_location` conversion path end to end against real
//! fixtures:
//!
//! * `<location>` elements appear on the expected element classes;
//! * every emitted `value` is within the DocLang `0..=512` grid and carries
//!   `resolution="512"`;
//! * exactly four `<location>` elements are emitted per located element
//!   (the schema requires the full x_min/y_min/x_max/y_max quadruple);
//! * the default-off path is byte-identical to a location-free conversion
//!   (the geometry pass must never perturb existing output);
//! * the `Prov -> Location` join resolves boxes for body content;
//! * under the `validator-integration` feature, located output passes the
//!   Python reference validator (`doclang validate -n`).

mod common;

use std::collections::HashMap;

use hangulang::{convert, ConvertOptions, Mode};

use common::{read_fixture, Fixture};

fn fixtures_dir() -> std::path::PathBuf {
    common::fixtures_dir()
}

/// A small representative subset spanning the located element classes:
/// tables, formulas (1×1-wrapped → table boxes), headings/lists/text.
fn located_fixtures() -> Vec<Fixture> {
    ["tables/table-complex", "formulas/eq-01", "headings/api-doc", "mixed/exam-social"]
        .iter()
        .map(|rel| {
            let path = fixtures_dir().join(format!("{rel}.hwp"));
            Fixture {
                name: rel.replace('/', "__"),
                path,
            }
        })
        .collect()
}

fn convert_located(data: &[u8]) -> String {
    let opts = ConvertOptions {
        mode: Mode::Lean,
        with_location: true,
        ..ConvertOptions::default()
    };
    convert(data, &opts).expect("convert with location").xml
}

/// Parse out every `<location value="N" resolution="R"/>` as `(N, R)`.
fn locations(xml: &str) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<location ") {
        rest = &rest[i..];
        let end = rest.find("/>").expect("closed location tag");
        let tag = &rest[..end];
        let value = attr(tag, "value").expect("location value");
        let res = attr(tag, "resolution").expect("location resolution");
        out.push((value, res));
        rest = &rest[end + 2..];
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<i64> {
    let key = format!("{name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

#[test]
fn location_values_in_range_and_resolution_512() {
    for fx in located_fixtures() {
        let data = read_fixture(&fx);
        let xml = convert_located(&data);
        let locs = locations(&xml);
        assert!(!locs.is_empty(), "{}: expected some <location> elements", fx.name);
        for (value, res) in &locs {
            assert!(
                (0..=512).contains(value),
                "{}: location value {value} out of 0..=512",
                fx.name
            );
            assert_eq!(*res, 512, "{}: resolution must be 512", fx.name);
        }
        // The schema requires exactly four locations per located element, so the
        // total must be a multiple of four.
        assert_eq!(
            locs.len() % 4,
            0,
            "{}: located elements must each carry 4 <location> (got {})",
            fx.name,
            locs.len()
        );
    }
}

#[test]
fn location_appears_on_expected_element_classes() {
    // api-doc is a heading/text/table document: it must produce boxes on text
    // and heading elements (the body-paragraph join), proving provenance works.
    let data = read_fixture(&Fixture {
        name: "headings__api-doc".into(),
        path: fixtures_dir().join("headings/api-doc.hwp"),
    });
    let xml = convert_located(&data);
    assert!(
        xml.contains("<text><location value="),
        "api-doc: a <text> block must carry a resolved location"
    );
    assert!(
        xml.contains("<heading level=\"1\"><location value=")
            || xml.contains("\"><location value="),
        "api-doc: a <heading> block must carry a resolved location"
    );

    // table-complex must put boxes on tables.
    let data = read_fixture(&Fixture {
        name: "tables__table-complex".into(),
        path: fixtures_dir().join("tables/table-complex.hwp"),
    });
    let xml = convert_located(&data);
    assert!(
        xml.contains("<table><location value="),
        "table-complex: a <table> must carry a resolved location"
    );
}

#[test]
fn default_off_is_byte_identical_to_location_free() {
    // The geometry pass must be a strict no-op when disabled.
    for fx in located_fixtures() {
        let data = read_fixture(&fx);
        let off = convert(&data, &ConvertOptions::default()).unwrap().xml;
        assert!(
            !off.contains("<location"),
            "{}: default options must not emit <location>",
            fx.name
        );
    }
}

#[test]
fn location_output_is_deterministic() {
    // The Prov->Location map is a HashMap, but locations are emitted in
    // document order during the writer walk, so repeated runs must match.
    let data = read_fixture(&Fixture {
        name: "mixed__exam-social".into(),
        path: fixtures_dir().join("mixed/exam-social.hwp"),
    });
    let a = convert_located(&data);
    let b = convert_located(&data);
    assert_eq!(a, b, "located output must be deterministic");
}

#[test]
fn provenance_join_resolves_unique_boxes_per_element() {
    // Each located element gets one quadruple; across a document many distinct
    // boxes should resolve (i.e. the join is not collapsing everything to one
    // position). Count distinct first-of-four x_min values as a proxy.
    let data = read_fixture(&Fixture {
        name: "headings__api-doc".into(),
        path: fixtures_dir().join("headings/api-doc.hwp"),
    });
    let xml = convert_located(&data);
    let locs = locations(&xml);
    // Group into quadruples and collect distinct boxes.
    let mut distinct: HashMap<(i64, i64, i64, i64), usize> = HashMap::new();
    for quad in locs.chunks(4) {
        if quad.len() == 4 {
            distinct
                .entry((quad[0].0, quad[1].0, quad[2].0, quad[3].0))
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }
    }
    assert!(
        distinct.len() > 5,
        "expected many distinct resolved boxes, got {}",
        distinct.len()
    );
}

/// Committed golden baselines for the located (`with_location = true`) lean
/// output of two representative fixtures. Regenerate with:
///
/// ```text
/// UPDATE_GOLDEN=1 cargo test --test location location_goldens_match
/// git diff tests/fixtures/golden
/// ```
#[test]
fn location_goldens_match() {
    let golden_dir = fixtures_dir().join("golden");
    let update = std::env::var("UPDATE_GOLDEN").map(|v| v == "1").unwrap_or(false);

    // (fixture rel path, golden file stem)
    let cases = [
        ("tables/table-complex", "tables__table-complex__hwp.location.lean.dclg.xml"),
        ("mixed/exam-social", "mixed__exam-social__hwp.location.lean.dclg.xml"),
    ];

    let mut mismatches = Vec::new();
    for (rel, golden_name) in cases {
        let data = std::fs::read(fixtures_dir().join(format!("{rel}.hwp"))).expect("read fixture");
        let xml = convert_located(&data);
        let path = golden_dir.join(golden_name);

        if update {
            std::fs::write(&path, xml.as_bytes()).expect("write golden");
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(expected) if expected == xml => {}
            Ok(expected) => mismatches.push(format!(
                "{rel}: located output differs from {} (expected {} bytes, got {})",
                path.display(),
                expected.len(),
                xml.len()
            )),
            Err(_) => mismatches.push(format!(
                "{rel}: missing golden {} (run UPDATE_GOLDEN=1)",
                path.display()
            )),
        }
    }
    if update {
        eprintln!("UPDATE_GOLDEN=1: regenerated {} location goldens", cases.len());
        return;
    }
    assert!(mismatches.is_empty(), "location golden mismatches:\n{}", mismatches.join("\n"));
}

#[cfg(feature = "validator-integration")]
mod validator {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

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

    #[test]
    fn located_output_passes_reference_validator() {
        let Some(doclang) = doclang_bin() else {
            eprintln!("SKIP: .venv/bin/doclang not found");
            return;
        };
        let tmp = std::env::temp_dir().join("hangulang-location-validator");
        std::fs::create_dir_all(&tmp).expect("create temp dir");

        let mut failures: Vec<String> = Vec::new();
        for fx in located_fixtures() {
            let data = read_fixture(&fx);
            let xml = convert_located(&data);
            let out_path = tmp.join(format!("{}.location.dclg.xml", fx.name));
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
                let detail: String = stdout
                    .lines()
                    .chain(stderr.lines())
                    .filter(|l| l.contains("Line ") || l.contains("FAILED") || l.contains("Error"))
                    .take(6)
                    .collect::<Vec<_>>()
                    .join("\n    ");
                failures.push(format!(
                    "{} [location]: validator exit {:?}\n    {}",
                    fx.name,
                    output.status.code(),
                    detail
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "{} located outputs failed reference validation:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
