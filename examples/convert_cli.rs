//! Command-line HWP → DocLang converter.
//!
//! # Usage
//!
//! ```text
//! cargo run --example convert_cli -- <input.hwp|.hwpx> [--mode lean|preserve] [--location] [-o out.dclg.xml]
//! ```
//!
//! Defaults:
//! - `--mode lean`
//! - `--location` off (no v2 `<location>` bounding boxes)
//! - output path = input file stem + `.dclg.xml` (in the same directory)
//!
//! The loss report summary is printed to stderr (count by kind).
//! Exit code is non-zero on any error.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process;

use hangulang::{convert, ConvertOptions, LossKind, Mode};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let parsed = match parse_args(&args[1..]) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            eprintln!("{}", usage());
            process::exit(2);
        }
    };

    // Read input bytes.
    let data = match std::fs::read(&parsed.input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", parsed.input.display());
            process::exit(1);
        }
    };

    // Build options.
    let opts = ConvertOptions {
        mode: parsed.mode,
        with_location: parsed.with_location,
        ..ConvertOptions::default()
    };

    // Run the converter.
    let outcome = match convert(&data, &opts) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    // Write XML output.
    let output_path = parsed
        .output
        .unwrap_or_else(|| default_output_path(&parsed.input));

    if let Err(e) = std::fs::write(&output_path, &outcome.xml) {
        eprintln!("error: cannot write '{}': {e}", output_path.display());
        process::exit(1);
    }

    // Print loss report summary to stderr.
    if outcome.loss.is_empty() {
        eprintln!("loss: none");
    } else {
        eprintln!("loss: {} entries", outcome.loss.len());

        // Tally by kind label.
        let mut counts: HashMap<String, usize> = HashMap::new();
        for entry in outcome.loss.iter() {
            let label = loss_kind_label(&entry.kind);
            *counts.entry(label).or_insert(0) += 1;
        }

        // Print in sorted key order for deterministic output.
        let mut pairs: Vec<_> = counts.into_iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        for (label, count) in pairs {
            eprintln!("  {label}: {count}");
        }
    }

    eprintln!("output: {}", output_path.display());
}

// ---------------------------------------------------------------------------
// Argument parsing
// ---------------------------------------------------------------------------

struct Args {
    input: PathBuf,
    mode: Mode,
    output: Option<PathBuf>,
    with_location: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut input: Option<PathBuf> = None;
    let mut mode = Mode::Lean;
    let mut output: Option<PathBuf> = None;
    let mut with_location = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--location" => {
                with_location = true;
            }
            "--mode" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| "--mode requires an argument (lean|preserve)".to_string())?;
                mode = match val.as_str() {
                    "lean" => Mode::Lean,
                    "preserve" => Mode::Preserve,
                    other => {
                        return Err(format!("unknown mode '{other}'; expected lean or preserve"))
                    }
                };
            }
            "-o" | "--output" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| "-o requires a path argument".to_string())?;
                output = Some(PathBuf::from(val));
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unknown flag '{flag}'"));
            }
            positional => {
                if input.is_some() {
                    return Err("too many positional arguments; expected exactly one input file"
                        .to_string());
                }
                input = Some(PathBuf::from(positional));
            }
        }
        i += 1;
    }

    let input = input.ok_or_else(|| "missing required argument: <input.hwp|.hwpx>".to_string())?;
    Ok(Args { input, mode, output, with_location })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derive the default output path from the input path.
///
/// `path/to/doc.hwp` → `path/to/doc.dclg.xml`
/// `doc` (no extension) → `doc.dclg.xml`
fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let mut out = input.with_file_name(format!("{stem}.dclg.xml"));
    // Ensure the parent directory is retained even if `with_file_name` drops it.
    if let Some(parent) = input.parent() {
        out = parent.join(format!("{stem}.dclg.xml"));
    }
    out
}

/// Map a [`LossKind`] to a short display label for the summary table.
fn loss_kind_label(kind: &LossKind) -> String {
    match kind {
        LossKind::FontInfo => "font_info".to_string(),
        LossKind::CharColor => "char_color".to_string(),
        LossKind::NamedStyle => "named_style".to_string(),
        LossKind::SectionSettings => "section_settings".to_string(),
        LossKind::FloatingObject => "floating_object".to_string(),
        LossKind::TextBox => "text_box".to_string(),
        LossKind::TrackChanges => "track_changes".to_string(),
        LossKind::FormulaFallback => "formula_fallback".to_string(),
        LossKind::Caption => "caption".to_string(),
        LossKind::Other(s) => format!("other:{s}"),
    }
}

fn usage() -> &'static str {
    "usage: convert_cli <input.hwp|.hwpx> [--mode lean|preserve] [--location] [-o out.dclg.xml]"
}
