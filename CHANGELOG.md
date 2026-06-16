# Changelog

All notable changes to this project will be documented in this file.

This project follows semantic versioning for the Rust API once published. The
semantic payload schema is versioned separately with `schema_version`.

## Unreleased

- Added DocLang XML, semantic payload, Markdown, and resource export surfaces.
- Added rhwp render-tree based optional layout bounding boxes.
- Added MIT license file and release preparation metadata.
- Rescued previously dropped text from `Hyperlink` (HWP3 display text), `Ruby`,
  and `CharOverlap` controls as plain blocks instead of discarding it.
- Recorded a loss entry when outline heading levels deeper than 6 are clamped.
- Fixed font-info loss reporting to inspect all seven language slots instead of
  only the Korean (default) slot.
- Added inline hyperlink support: hyperlink fields are emitted as `Inline::Href`
  (`<href uri="…">…</href>` in XML, `[anchor](uri)` in Markdown, `kind: "href"`
  with a `uri` field in the semantic payload), resolved from the anchor span via
  the paragraph's `field_ranges`.
- Expanded EqEdit→LaTeX coverage with more no-argument symbols (perp, parallel,
  langle/rangle, setminus, long/hook arrows, …) and over/under decorations
  (overline, underline, widehat, ddot, …).
- Interleaved in-text flow objects (tables, pictures, formulas) at their true
  character position within ordinary paragraphs instead of appending them.
- Reported layout-pass failures through `LossReport` (`location-unavailable`,
  `location-page-layout-failed`) instead of returning a silent empty map.
- Recorded table-cell collisions (`table-cell-collision`) when clamped anchors
  overlap, and capped the OTSL grid size to avoid OOM on malformed huge tables.
