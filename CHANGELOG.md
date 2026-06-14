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
- Documented deferred conversion gaps in `docs/v2-known-limitations.md`.
