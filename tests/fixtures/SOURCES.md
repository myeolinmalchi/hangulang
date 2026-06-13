# Test Fixture Sources

All binary fixtures in this directory are real HWP 5.0 / HWPX documents used to
exercise the converter end-to-end (parse → Semantic IR → DocLang v0.6 XML) and
to drive the golden-file and validator integration tests.

## Provenance & license

Every fixture is copied verbatim from the **rhwp** parser project's own sample
corpus:

- Repository: <https://github.com/edwardkim/rhwp>
- Commit pinned: `bc38ff55a7e8acb65aebebe237dca0542480d381`
  (the exact rev this crate depends on — see `Cargo.toml`).
- License: **MIT** (`Copyright (c) 2025-2026 Edward Kim`). The MIT license
  permits redistribution and modification, including bundling these sample
  documents in a downstream test suite.

These samples are the upstream parser's regression corpus; reusing them keeps
our converter tests aligned with the exact documents the parser core is known
to handle. They were *not* LFS-stored (the repo only LFS-tracks
`pdf-large/**/*.pdf`), so they are plain git blobs reproduced here as-is.

> Note on KOGL / public-sector documents: the plan also contemplated pulling
> Korean public-sector HWP files (korea.kr / law.go.kr / data.go.kr) under
> KOGL Type 1. That path was **not needed** — the MIT-licensed rhwp corpus
> already provides ≥1 real document for every v1 element class (and several
> genuine HWP5↔HWPX content pairs), which is both legally cleaner and more
> directly relevant to the parser core we build on. If broader public-sector
> coverage is wanted later, add those files under a new subdirectory and record
> their source URL + KOGL type here.

## Inventory (by element class)

| File | Upstream path | Elements exercised |
|------|---------------|--------------------|
| `paragraphs/para-001.hwp` | `samples/para-001.hwp` | paragraphs, inline runs, mixed Korean/Hanja text |
| `tables/table-001.hwp` | `samples/table-001.hwp` | table, header cells (`ched`), col-spans (`lcel`) |
| `tables/table-complex.hwp` | `samples/table-complex.hwp` | complex merges, nested content, floating shapes (→ `<group><custom>` in preserve) |
| `tables/inner-table-01.hwp` | `samples/inner-table-01.hwp` | nested table (table inside cell) |
| `formulas/eq-01.hwp` | `samples/eq-01.hwp` | EqEdit formulas → `<formula>` LaTeX (with fallback) |
| `footnotes/footnote-01.hwp` | `samples/footnote-01.hwp` | `<footnote>` (recursive content) |
| `footnotes/endnote-01.hwp` | `samples/endnote-01.hwp` | endnotes (footnote vocabulary) |
| `pictures/hwp-img-001.hwp` | `samples/hwp-img-001.hwp` | embedded image → `<picture><src data:…base64>` |
| `headerfooter/pic-in-head-01.hwp` | `samples/pic-in-head-01.hwp` | page header/footer + image in header |
| `lists/number-bullet.hwp` | `rhwp-studio/public/samples/number-bullet.hwp` | `<list>`/`<ldiv>` (numbered + bullet) |
| `lists/para-head-num-2.hwp` | `rhwp-studio/public/samples/para-head-num-2.hwp` | numbered-paragraph (문단 번호) levels → a single `<list class="ordered">` (these are *number* paragraph heads, **not** outline headings; the converter emits `<list>`, never `<heading>`, for them) |
| `headings/api-doc.hwp` | `samples/hwpctl_API_v2.4.hwp` | outline-style `<heading>` (levels 1–2, real Korean titles) — the *only* corpus source that exercises `<heading>` |
| `mixed/sub-superscript.hwp` | `samples/hwp3-sample-hwp5.hwp` | inline `<subscript>` (Wᵢ, Cᵢ) **and** `<superscript>` (footnote markers); also carries a `<page_footer>` |
| `mixed/exam-social.hwp` | `samples/exam_social.hwp` | 시험지 (2025 수능 사회탐구): `<page_footer>` |
| `mixed/exam-math.hwp` | `samples/exam_math.hwp` | 시험지 (수능 수학): multi-column layout → `<thread>` (start + continuation) |
| `mixed/strikethrough-synth.hwpx` | *synthetic* — see below | `<strikethrough>` (the one v1 element absent from the entire upstream corpus) |
| `pairs/para-001.hwp` + `pairs/para-001.hwpx` | `samples/para-001.hwp`, `samples/hwpx/para-001.hwpx` | **HWP5↔HWPX equivalence pair** (same content, both formats) |
| `pairs/test-image.hwp` + `pairs/test-image.hwpx` | `samples/test-image.hwp`, `samples/test-image.hwpx` | **HWP5↔HWPX equivalence pair** with an embedded image |

## Equivalence pairs

`pairs/para-001.{hwp,hwpx}` and `pairs/test-image.{hwp,hwpx}` are the same source
document saved in both the binary (CFB) HWP5 format and the zipped-XML HWPX
format. The converter produces **byte-identical** DocLang output for each pair
in both `lean` and `preserve` modes — this is asserted by `tests/equivalence.rs`.

## Synthetic fixture: `mixed/strikethrough-synth.hwpx`

`<strikethrough>` (취소선) is the only DocLang v1 inline element that **no
document in the entire upstream rhwp corpus exercises** — a full batch-conversion
of all ~320 samples (both `lean` and `preserve`) produced zero `<strikethrough>`
tags. (The HWPX parser is deliberately conservative here: it only treats a
charPr's `<hh:strikeout shape="…"/>` as a real strike for a whitelist of line
shapes — `SOLID`, `DASH`, `WAVE`, … — because Hancom's exporter spuriously
stamps `shape="3D"` on ordinary body text. None of the corpus docs carry a
whitelisted strike shape on any run.)

To close that coverage gap honestly, this fixture is **synthesised**, not taken
from the corpus:

- **Base**: `samples/hwpx/blank_hwpx.hwpx` from the same rhwp corpus (MIT,
  commit `bc38ff55`) — a minimal, valid single-section HWPX.
- **Edit**: one extra `<hh:charPr id="7">` was added to `Contents/header.xml`
  (a clone of `charPr id="0"` plus `<hh:strikeout shape="SOLID" color="#000000"/>`,
  with `charProperties/@itemCnt` bumped 7→8), and one paragraph was appended to
  `Contents/section0.xml` containing a run with `charPrIDRef="7"` so the struck
  text actually references the new char shape. The container was re-zipped with
  `mimetype` stored first (uncompressed), as the HWPX/OPC spec requires.
- **Result**: converts (both modes) to
  `<text>이 문장은 <strikethrough>취소선 그어진 텍스트</strikethrough>입니다.</text>`
  and passes the reference validator.

The edit only adds a strike-marked run; it is the smallest change that makes a
valid HWPX emit `<strikethrough>`. If a corpus document that genuinely uses a
whitelisted strike shape is found later, this synthetic fixture can be replaced.

## Converter fixes prompted by these fixtures

The new exam fixtures surfaced two pre-existing writer bugs that emitted
**schema-invalid** DocLang v0.6 (caught by the reference validator). Both are
fixed in `src/writer/mod.rs`; existing goldens are unaffected because no prior
fixture exercised either path:

- **`page_header`/`page_footer` `apply` attribute** — the writer emitted
  `apply="odd|even|first"`, but v0.6's `component_with_semantic_seq` has no such
  attribute. The page-scope is now dropped from the output and recorded as a
  `SectionSettings` loss instead. (`exam-social`, `sub-superscript`.)
- **standalone `<thread>` blocks** — the writer emitted
  `<thread id="col-0"/>` / `<thread … continuation="true"/>` as top-level
  siblings, but v0.6's `<thread>` is an *element-head* member with a required
  `thread_id` of type `positiveInteger` (no `id`, no `continuation`). Each
  boundary is now anchored to an empty `<group><thread thread_id="N"/></group>`;
  the start/continuation distinction (unrepresentable in v0.6) is recorded as a
  loss. (`exam-math`.)
