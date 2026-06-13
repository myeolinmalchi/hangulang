//! Inline formatting-run splitting (Phase A — inline).
//!
//! Lowers a single rhwp [`Paragraph`] into a sequence of crate-IR [`Inline`]
//! nodes: plain/styled text spans, tabs, and line breaks.  Inline *objects*
//! (tables, pictures, equations, footnotes, …) are **not** emitted here — they
//! are handled at block level via `Paragraph.controls` in later tasks.
//!
//! # `CharShapeRef.start_pos` unit — VERIFIED (Critic N1)
//!
//! `start_pos` is a **UTF-16 code-unit stream offset** measured from the start
//! of the paragraph's PARA_TEXT stream (NOT a visible-char index).
//!
//! Evidence from the authoritative rhwp source (`/tmp/rhwp-src/`):
//!
//! 1. The parser reads `start_pos` verbatim from the 8-byte PARA_CHAR_SHAPE
//!    record entries (`u32 start_pos` + `u32 char_shape_id`):
//!    `src/parser/body_text.rs:409-423` (`parse_para_char_shape`).
//! 2. The same scale is used for `char_offsets`: each text char records
//!    `code_unit_pos = (pos / 2)` where `pos` is the byte offset into the
//!    UTF-16LE PARA_TEXT stream — i.e. the UTF-16 code-unit index:
//!    `src/parser/body_text.rs:275,282,365,374`.  Surrogate-pair chars push a
//!    single `char_offsets` entry but advance `pos` by 4 bytes (= +2 code
//!    units), so a shape that begins after an emoji has a `start_pos` two
//!    higher than the char index (`body_text.rs:360-371`).
//! 3. rhwp's own renderer resolves runs with Interpretation A and explicitly
//!    documents `start_pos` as a "UTF-16 stream offset", finding the first
//!    `char_offsets` entry `>= start_pos` as the run's first visible char:
//!    `src/renderer/composer.rs:800-825` (Task #915 — an earlier
//!    "visible-char-index" interpretation, #884, was reverted as a bug).
//!
//! This module mimics Interpretation A exactly: `char_offsets` maps each
//! visible char index → its UTF-16 stream offset, and a run boundary at
//! `start_pos` becomes the visible char index of the first `char_offsets`
//! entry `>= start_pos`.

use rhwp::model::document::DocInfo;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::style::CharShape;

use crate::ir::{Inline, StyleFlags};
use crate::loss::report::{LossEntry, LossKind, LossReport};

use super::resources;

/// Unicode OBJECT REPLACEMENT CHARACTER.
///
/// The rhwp HWP3 path inserts this as a placeholder for inline objects (the
/// HWP5 path leaves a `char_offsets` gap instead).  The actual objects are
/// emitted from `Paragraph.controls`, so we strip the marker here to avoid a
/// stray "?" glyph in the text stream.
const OBJECT_REPLACEMENT: char = '\u{FFFC}';

/// Lower one paragraph's text + char-shape runs into a sequence of [`Inline`]
/// nodes.
///
/// `loss` accumulates lean-mode loss entries; this function records at most one
/// [`LossKind::FontInfo`] and one [`LossKind::CharColor`] per paragraph (never
/// per run) using `location` to identify the source paragraph.
pub(crate) fn extract_inlines(
    para: &Paragraph,
    doc_info: &DocInfo,
    location: &str,
    loss: &mut LossReport,
) -> Vec<Inline> {
    let chars: Vec<char> = para.text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    // Per-paragraph loss de-duplication flags.
    let mut font_loss_recorded = false;
    let mut color_loss_recorded = false;

    // Build run boundaries as visible-char indices.  `boundaries[i]` is the
    // first char index covered by `char_shapes[i]`; the run extends up to the
    // next boundary (or end of text).
    let runs = build_runs(para, chars.len());

    // Convert each run into (style, text-slice) then walk the slice splitting on
    // control chars (tab / line-break / object marker).
    let mut inlines: Vec<Inline> = Vec::new();
    let mut pending_text = String::new();
    let mut pending_style = StyleFlags::default();

    let flush = |inlines: &mut Vec<Inline>, text: &mut String, style: &StyleFlags| {
        if text.is_empty() {
            return;
        }
        let node = if style.is_plain() {
            Inline::Text(std::mem::take(text))
        } else {
            Inline::Styled(*style, vec![Inline::Text(std::mem::take(text))])
        };
        push_merged(inlines, node);
    };

    for run in &runs {
        let style = match run.char_shape_id.and_then(|id| resources::char_shape(doc_info, id)) {
            Some(cs) => {
                record_char_shape_loss(
                    cs,
                    doc_info,
                    location,
                    loss,
                    &mut font_loss_recorded,
                    &mut color_loss_recorded,
                );
                style_flags_from(cs)
            }
            None => StyleFlags::default(),
        };

        for &c in &chars[run.start..run.end] {
            match c {
                '\t' => {
                    flush(&mut inlines, &mut pending_text, &pending_style);
                    push_merged(&mut inlines, Inline::Tab);
                }
                '\n' => {
                    flush(&mut inlines, &mut pending_text, &pending_style);
                    push_merged(&mut inlines, Inline::LineBreak);
                }
                OBJECT_REPLACEMENT => {
                    // Object placeholder — handled via controls[] elsewhere.
                    flush(&mut inlines, &mut pending_text, &pending_style);
                }
                _ => {
                    // If the style changed since the last buffered char, flush
                    // the previous span first so each span carries one style.
                    if !pending_text.is_empty() && pending_style != style {
                        flush(&mut inlines, &mut pending_text, &pending_style);
                    }
                    pending_style = style;
                    pending_text.push(c);
                }
            }
        }
    }
    flush(&mut inlines, &mut pending_text, &pending_style);

    inlines
}

/// A contiguous formatting run expressed as a half-open visible-char range.
struct Run {
    start: usize,
    end: usize,
    /// `char_shape_id` for this run, or `None` if no shape applies.
    char_shape_id: Option<u32>,
}

/// Compute formatting-run boundaries (visible-char indices).
///
/// For each `CharShapeRef`, the run begins at the first `char_offsets` entry
/// whose UTF-16 offset is `>= start_pos` (Interpretation A, mirroring rhwp's
/// `composer.rs:822-825`).  Runs are emitted in char-index order; if no shape
/// covers char 0 the leading text uses `None` (plain).
fn build_runs(para: &Paragraph, char_len: usize) -> Vec<Run> {
    // Map each char_shape's start_pos → visible char index, dedup, sort.
    let mut starts: Vec<(usize, u32)> = Vec::with_capacity(para.char_shapes.len());
    for cs in &para.char_shapes {
        let idx = para
            .char_offsets
            .iter()
            .position(|&off| off >= cs.start_pos)
            .unwrap_or(char_len);
        if idx < char_len {
            starts.push((idx, cs.char_shape_id));
        }
    }
    starts.sort_by_key(|&(idx, _)| idx);
    // On duplicate start index keep the last entry (most recent CharShapeRef),
    // matching rhwp's reverse-dedup in composer.rs:837-841.
    let mut deduped: Vec<(usize, u32)> = Vec::with_capacity(starts.len());
    for (idx, id) in starts {
        if let Some(last) = deduped.last_mut() {
            if last.0 == idx {
                last.1 = id;
                continue;
            }
        }
        deduped.push((idx, id));
    }

    let mut runs: Vec<Run> = Vec::new();

    // Leading text not covered by any shape → plain run.
    let first_start = deduped.first().map(|&(idx, _)| idx).unwrap_or(char_len);
    if first_start > 0 {
        runs.push(Run {
            start: 0,
            end: first_start,
            char_shape_id: None,
        });
    }

    for (i, &(start, id)) in deduped.iter().enumerate() {
        let end = deduped.get(i + 1).map(|&(next, _)| next).unwrap_or(char_len);
        if start < end {
            runs.push(Run {
                start,
                end,
                char_shape_id: Some(id),
            });
        }
    }

    if runs.is_empty() {
        runs.push(Run {
            start: 0,
            end: char_len,
            char_shape_id: None,
        });
    }

    runs
}

/// Derive the six DocLang-expressible flags from a resolved [`CharShape`].
fn style_flags_from(cs: &CharShape) -> StyleFlags {
    use rhwp::model::style::UnderlineType;
    StyleFlags {
        bold: cs.bold,
        italic: cs.italic,
        underline: cs.underline_type != UnderlineType::None,
        strike: cs.strikethrough,
        superscript: cs.superscript,
        subscript: cs.subscript,
    }
}

/// Record lean-mode loss for properties a [`CharShape`] carries that DocLang
/// cannot express (non-default text colour, font face/size).
///
/// At most one entry of each kind is recorded per paragraph; the boolean flags
/// guard against per-run spam.
fn record_char_shape_loss(
    cs: &CharShape,
    doc_info: &DocInfo,
    location: &str,
    loss: &mut LossReport,
    font_recorded: &mut bool,
    color_recorded: &mut bool,
) {
    // text_color is ColorRef = u32; default/black is 0.  Any non-zero value is
    // colour information DocLang cannot express.
    if !*color_recorded && cs.text_color != 0 {
        loss.push(LossEntry {
            kind: LossKind::CharColor,
            location: location.to_string(),
            detail: format!("text_color=0x{:06X}", cs.text_color & 0x00FF_FFFF),
        });
        *color_recorded = true;
    }

    if !*font_recorded {
        // font_ids[0] is the 한글 (default) face; resolve a human-readable name
        // when available.  Any present font face / non-zero size is loss.
        let font = resources::font_name(doc_info, 0, cs.font_ids[0]);
        let has_font = font.map(|n| !n.is_empty()).unwrap_or(false);
        if has_font || cs.base_size != 0 {
            let detail = match font {
                Some(name) if !name.is_empty() => {
                    format!("font={}, base_size={}", name, cs.base_size)
                }
                _ => format!("font_id={}, base_size={}", cs.font_ids[0], cs.base_size),
            };
            loss.push(LossEntry {
                kind: LossKind::FontInfo,
                location: location.to_string(),
                detail,
            });
            *font_recorded = true;
        }
    }
}

/// Append `node`, merging it into the previous node when both are `Text`/plain
/// or identically-`Styled`, so adjacent runs with identical formatting collapse.
fn push_merged(inlines: &mut Vec<Inline>, node: Inline) {
    if let Some(last) = inlines.last_mut() {
        match (last, &node) {
            (Inline::Text(prev), Inline::Text(next)) => {
                prev.push_str(next);
                return;
            }
            (Inline::Styled(p_flags, p_children), Inline::Styled(n_flags, _))
                if p_flags == n_flags =>
            {
                if let Inline::Styled(_, n_children) = node {
                    // Merge inner text where possible.
                    for child in n_children {
                        push_merged(p_children, child);
                    }
                }
                return;
            }
            _ => {}
        }
    }
    inlines.push(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rhwp::model::document::DocInfo;
    use rhwp::model::paragraph::{CharShapeRef, Paragraph};
    use rhwp::model::style::CharShape;

    /// Build a paragraph from `text`, computing `char_offsets` as the UTF-16
    /// stream offsets (mirroring the parser).  `shapes` are `(start_pos,
    /// char_shape_id)` pairs in UTF-16 code-unit space.
    fn make_para(text: &str, shapes: &[(u32, u32)]) -> Paragraph {
        let mut char_offsets = Vec::new();
        let mut off: u32 = 0;
        for c in text.chars() {
            char_offsets.push(off);
            off += if (c as u32) > 0xFFFF { 2 } else { 1 };
        }
        Paragraph {
            text: text.to_string(),
            char_offsets,
            char_shapes: shapes
                .iter()
                .map(|&(start_pos, char_shape_id)| CharShapeRef {
                    start_pos,
                    char_shape_id,
                })
                .collect(),
            ..Default::default()
        }
    }

    fn bold_shape() -> CharShape {
        CharShape {
            bold: true,
            ..Default::default()
        }
    }

    fn doc_info(shapes: Vec<CharShape>) -> DocInfo {
        DocInfo {
            char_shapes: shapes,
            ..Default::default()
        }
    }

    #[test]
    fn single_run_plain() {
        let para = make_para("hello", &[(0, 0)]);
        let di = doc_info(vec![CharShape::default()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(out, vec![Inline::Text("hello".to_string())]);
        assert!(loss.is_empty());
    }

    #[test]
    fn single_run_bold() {
        let para = make_para("hello", &[(0, 0)]);
        let di = doc_info(vec![bold_shape()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![Inline::Styled(
                StyleFlags {
                    bold: true,
                    ..Default::default()
                },
                vec![Inline::Text("hello".to_string())]
            )]
        );
    }

    #[test]
    fn multi_run_style_change_mid_text() {
        // "ab" plain, "cd" bold. start_pos 2 → char index 2.
        let para = make_para("abcd", &[(0, 0), (2, 1)]);
        let di = doc_info(vec![CharShape::default(), bold_shape()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![
                Inline::Text("ab".to_string()),
                Inline::Styled(
                    StyleFlags {
                        bold: true,
                        ..Default::default()
                    },
                    vec![Inline::Text("cd".to_string())]
                ),
            ]
        );
    }

    #[test]
    fn surrogate_pair_utf16_offset_conversion() {
        // "A😀B": 😀 (U+1F600) is one char but two UTF-16 code units.
        // char_offsets = [0, 1, 3].  A bold run beginning AFTER the emoji must
        // use start_pos = 3 (UTF-16), which maps to char index 2 ("B").
        let para = make_para("A😀B", &[(0, 0), (3, 1)]);
        let di = doc_info(vec![CharShape::default(), bold_shape()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![
                Inline::Text("A😀".to_string()),
                Inline::Styled(
                    StyleFlags {
                        bold: true,
                        ..Default::default()
                    },
                    vec![Inline::Text("B".to_string())]
                ),
            ]
        );
    }

    #[test]
    fn surrogate_pair_boundary_at_emoji() {
        // Bold run begins AT the emoji: start_pos = 1 → char index 1.
        let para = make_para("A😀B", &[(0, 0), (1, 1)]);
        let di = doc_info(vec![CharShape::default(), bold_shape()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![
                Inline::Text("A".to_string()),
                Inline::Styled(
                    StyleFlags {
                        bold: true,
                        ..Default::default()
                    },
                    vec![Inline::Text("😀B".to_string())]
                ),
            ]
        );
    }

    #[test]
    fn adjacent_identical_runs_merged() {
        // Two consecutive bold runs (different char_shape_id but identical
        // flags) collapse into one Styled node.
        let para = make_para("abcd", &[(0, 0), (2, 1)]);
        let di = doc_info(vec![bold_shape(), bold_shape()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![Inline::Styled(
                StyleFlags {
                    bold: true,
                    ..Default::default()
                },
                vec![Inline::Text("abcd".to_string())]
            )]
        );
    }

    #[test]
    fn tab_and_linebreak_control_chars() {
        let para = make_para("a\tb\nc", &[(0, 0)]);
        let di = doc_info(vec![CharShape::default()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![
                Inline::Text("a".to_string()),
                Inline::Tab,
                Inline::Text("b".to_string()),
                Inline::LineBreak,
                Inline::Text("c".to_string()),
            ]
        );
    }

    #[test]
    fn object_replacement_char_stripped() {
        let para = make_para("a\u{FFFC}b", &[(0, 0)]);
        let di = doc_info(vec![CharShape::default()]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        // Marker removed; surrounding text merges.
        assert_eq!(out, vec![Inline::Text("ab".to_string())]);
    }

    #[test]
    fn empty_text() {
        let para = make_para("", &[]);
        let di = doc_info(vec![]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert!(out.is_empty());
        assert!(loss.is_empty());
    }

    #[test]
    fn no_char_shapes_defaults_plain() {
        let para = make_para("plain", &[]);
        let di = doc_info(vec![]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(out, vec![Inline::Text("plain".to_string())]);
    }

    #[test]
    fn color_and_font_loss_recorded_once_per_paragraph() {
        // Two runs, both carrying colour + font info; loss recorded once each.
        let cs0 = CharShape {
            text_color: 0x0000_00FF, // red-ish (BGR), non-zero
            base_size: 1000,
            font_ids: [1, 0, 0, 0, 0, 0, 0],
            ..Default::default()
        };
        let mut cs1 = cs0.clone();
        cs1.text_color = 0x00FF_0000;

        let font = rhwp::model::style::Font {
            name: "Nanum".to_string(),
            ..Default::default()
        };
        let mut di = doc_info(vec![cs0, cs1]);
        di.font_faces = vec![vec![rhwp::model::style::Font::default(), font]];

        let para = make_para("abcd", &[(0, 0), (2, 1)]);
        let mut loss = LossReport::new();
        let _ = extract_inlines(&para, &di, "s0/p0", &mut loss);

        let color_count = loss
            .iter()
            .filter(|e| e.kind == LossKind::CharColor)
            .count();
        let font_count = loss.iter().filter(|e| e.kind == LossKind::FontInfo).count();
        assert_eq!(color_count, 1, "colour loss must be recorded once");
        assert_eq!(font_count, 1, "font loss must be recorded once");
    }

    #[test]
    fn nested_flags_bold_italic() {
        let cs = CharShape {
            bold: true,
            italic: true,
            ..Default::default()
        };
        let di = doc_info(vec![cs]);
        let para = make_para("x", &[(0, 0)]);
        let mut loss = LossReport::new();
        let out = extract_inlines(&para, &di, "s0/p0", &mut loss);
        assert_eq!(
            out,
            vec![Inline::Styled(
                StyleFlags {
                    bold: true,
                    italic: true,
                    ..Default::default()
                },
                vec![Inline::Text("x".to_string())]
            )]
        );
    }
}
