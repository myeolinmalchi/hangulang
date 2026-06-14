# hangulang

`hangulang`은 **HWP 5.0** 및 **HWPX** 문서(한컴오피스 / 한글)를 위한
Rust 기반 semantic extraction toolkit입니다.

현재 primary exporter는 **[DocLang](https://github.com/doclang-project/doclang)
v0.6**입니다. DocLang은 LF AI & Data Foundation이 만든 AI 네이티브 · LLM
토크나이저 친화적 XML 문서 포맷입니다.

`hangulang`은 [`rhwp`](https://github.com/edwardkim/rhwp) 파서 코어 위에서
문서의 의미 구조(제목, 목록, 표, 인라인 서식, 수식, 각주, 머리말/꼬리말),
이미지 리소스, 손실 보고, 그리고 — 선택적으로 — 레이아웃 좌표(bounding box)를
안정적인 내부 IR로 낮춥니다. 현재 이 IR에서 DocLang XML, semantic payload,
Markdown, resource asset/URI 참조를 직접 생성합니다.

> **상태:** v0.1 — 활발히 개발 중. 현재 DocLang exporter 출력은 공식 DocLang
> 레퍼런스 검증기(`doclang validate`)로 검증됩니다. JSON/Markdown/resource
> exporter와 rhwp 기반 bbox join은 unit/golden 테스트로 검증합니다. 현재 매핑
> 범위는 [커버리지](#커버리지)를 참고하세요.

---

## 왜 만들었나

HWP/HWPX는 한국 공공기관, 법무, 교육, 기업 문서 워크플로에서 여전히 중요하지만,
구조화된 extraction 도구 생태계는 PDF/DOCX에 비해 얇습니다. 기존 HWP 추출기는
대개 plain text 또는 Markdown을 타깃으로 하며, 병합 셀, 제목 위계, 수식, 각주,
이미지, 레이아웃 provenance를 잃기 쉽습니다.

`hangulang`은 이 공백을 다음 역할로 채웁니다:

- **검증된 파서를 재사용합니다.** 부분적인 자체 재구현이 아니라
  `rhwp`(테스트 1,100개 이상, HWP 5.0 + HWPX 완전 지원) 위에 구축했습니다.
- **텍스트가 아니라 구조를 보존합니다.** 현재 DocLang exporter에서는 병합/중첩 표를
  [OTSL](https://github.com/doclang-project/doclang) 토큰으로, 개요 수준은 실제
  `<heading>`으로, 수식은 LaTeX로 변환됩니다.
- **`rhwp` 타입을 직접 노출하지 않습니다.** 저수준 parser model을 소비자에게
  넘기지 않고, 변환기/파이프라인에서 쓰기 쉬운 안정 IR과 출력 계약을 제공합니다.
- **버린 것을 보고합니다.** 모든 변환은 `LossReport`를 반환하므로, 호출자는
  어떤 정보(글꼴, 색상, 각주 등)가 손실됐는지 정확히 알 수 있습니다 — 조용한
  데이터 손실이 없습니다.
- **검증을 통과합니다.** 테스트 코퍼스 전체가 공식 `doclang validate`의
  XSD + Schematron 검사를 100% 통과합니다.

## 프로젝트 범위

`hangulang`은 `rhwp`의 대체제가 아니라, `rhwp` 위에 올라가는 제품화 레이어입니다.

| 레이어 | 책임 |
|--------|------|
| `rhwp` | HWP/HWPX 파일 포맷 파싱, 내부 문서 모델, 렌더 트리 제공 |
| `hangulang` Rust core | `rhwp` 모델을 semantic IR로 낮추고, 수식/표/이미지/각주/좌표/loss를 정규화 |
| Exporters | DocLang XML, semantic JSON payload, Markdown, asset references |
| `hangulang-python` | Python wheel, Pythonic API, typed error, optional integrations(계획) |
| Integrations | Docling, LangChain, LlamaIndex 등 외부 adapter(계획) |

`rhwp-python`이 저수준 Python binding이라면, `hangulang-python`은 바로 사용할 수
있는 문서 변환 API를 지향합니다. 즉 `rhwp`의 raw model/render tree를 Python에
그대로 노출하기보다는, 안정된 semantic payload와 export 결과를 제공합니다.

---

## 설치

최신 안정 버전 Rust 툴체인이 필요합니다(edition 2021, 1.94 기준 개발).

`rhwp`는 crates.io에 게시되어 있지 않으므로, 재현성을 위해 정확한 커밋으로 고정한
git 의존성으로 사용합니다:

```toml
[dependencies]
hangulang = { git = "https://github.com/myeolinmalchi/hangulang" }
```

> **빌드 참고:** `rhwp`의 네이티브 빌드는 SVG/PDF 렌더링 스택(`svg2pdf`,
> `usvg`, `pdf-writer` 등)을 transitive하게 컴파일합니다. 최초 빌드에 약 35초가
> 걸립니다. 이는 알려진 upstream 비용입니다 — [로드맵](#로드맵) 참고.

---

## 빠른 시작

### 라이브러리

```rust
use hangulang::{convert, ConvertOptions};

let data = std::fs::read("document.hwp")?;
let outcome = convert(&data, &ConvertOptions::default())?;

println!("{}", outcome.xml);          // DocLang v0.6 XML
for entry in outcome.loss.iter() {    // 표현할 수 없었던 정보
    eprintln!("{:?} @ {}: {}", entry.kind, entry.location, entry.detail);
}
```

Semantic payload와 Markdown은 DocLang XML을 거치지 않고 같은 IR에서 직접 생성합니다:

```rust
use hangulang::{convert_to_markdown, convert_to_payload, ConvertOptions};

let payload = convert_to_payload(&data, &ConvertOptions::default())?;
let markdown = convert_to_markdown(&data, &ConvertOptions::default())?;
```

`serde` feature를 켜면 pretty JSON 문자열도 바로 만들 수 있습니다:

```rust
let json = hangulang::convert_to_json(&data, &ConvertOptions::default())?;
```

이미지는 기본적으로 self-contained data URI로 들어가며, 파일로 분리해야 하는
파이프라인에서는 리소스 정책을 바꿀 수 있습니다:

```rust
use hangulang::{convert, ConvertOptions, ResourcePolicy};

let opts = ConvertOptions {
    resource_policy: ResourcePolicy::asset_dir("assets"),
    ..Default::default()
};
let outcome = convert(&data, &opts)?;

for asset in outcome.assets {
    std::fs::write(format!("assets/{}", asset.path), asset.data)?;
}
```

### 출력 API

| API | 출력 | 비고 |
|-----|------|------|
| `convert` | DocLang XML, assets, `LossReport` | 기존 primary exporter |
| `extract_semantic` | `SirDocument`, `LocationMap`, `LossReport` | exporter를 직접 만들 때 사용 |
| `convert_to_payload` | versioned `SemanticPayload` | Python wrapper / integration용 안정 계약 |
| `convert_to_json` | pretty JSON 문자열 | `serde` feature 필요 |
| `convert_to_markdown` | Markdown, assets, `LossReport` | DocLang XML을 거치지 않는 직접 exporter |

### CLI

```bash
cargo run --example convert_cli -- document.hwp --mode lean -o document.dclg.xml
```

```text
usage: convert_cli <input.hwp|.hwpx> [--mode lean|preserve] [--location] [-o out.dclg.xml]

  --mode lean      표준 DocLang 요소만 출력하고 손실을 보고 (기본값)
  --mode preserve  표현 불가한 HWP 속성을 네임스페이스 <custom> 요소로 보존
  --location       <location> bounding box(레이아웃 좌표)도 함께 출력
  -o <path>        출력 파일 (기본값: <입력파일명>.dclg.xml)
```

손실 보고 요약은 stderr로 출력되며, 오류 시 종료 코드는 0이 아닙니다.

---

## 모드

DocLang은 의도적으로 미니멀합니다(의미 + 좌표 + 읽기 순서; 글꼴/색상/스타일
없음). HWP는 훨씬 많은 정보를 담습니다. 두 모드가 이 간극을 처리합니다:

| 모드 | 출력 | 사용 시점 |
|------|------|-----------|
| `Lean` *(기본값)* | 순수 표준 DocLang. 표현 불가 정보 → `LossReport`에 기록 후 XML에서 제외. | 토큰 효율이 중요한 RAG / LLM 인제스천. |
| `Preserve` | 표현 불가한 HWP 속성을 `<custom ns="hwp:…">`로 보존. | 정보 손실이 없어야 하는 라운드트립 / 아카이빙. |

```rust
use hangulang::{convert, ConvertOptions, Mode};

let opts = ConvertOptions { mode: Mode::Preserve, ..Default::default() };
let outcome = convert(&data, &opts)?;
```

### 레이아웃 좌표 (`--location`)

`with_location`을 활성화하면, 변환기는 `rhwp::DocumentCore` 렌더 트리로 각
페이지를 배치(layout)하고, render node의 `(section, paragraph, control)`
provenance를 Hangulang IR의 `Prov`와 join합니다. DocLang XML에는
`<location value="N" resolution="512"/>` 네 개(`x_min, y_min, x_max, y_max`)를
붙이고, semantic payload에는 `page`, `bbox`, `resolution`, `status`를 기록합니다.
좌표는 page-relative 0–512 그리드로 정규화됩니다.

```rust
let opts = ConvertOptions { with_location: true, ..Default::default() };
```

비활성화 시(기본값) 출력은 좌표 없는 빌드와 byte-identical이며, 페이지 레이아웃
패스는 완전히 생략됩니다.

현재 bbox가 붙는 범위:

| IR 블록 | rhwp render node | 처리 방식 |
|---------|------------------|-----------|
| 문단 / 제목 / 목록 | `TextLine(section, para)` | 같은 paragraph의 line box를 첫 페이지 안에서 union |
| 최상위 표 | `Table(section, para, control)` | control provenance로 join; 1x1 wrapper flattening 케이스 허용 |
| 본문 이미지 | `Image(section, para, control)` | 셀/머리말/꼬리말 내부 이미지는 제외 |
| 본문 수식 | `Equation(section, para, control)` | 셀/각주 내부 수식은 제외 |
| 머리말 / 꼬리말 | `Header` / `Footer` group | section의 header/footer control에 연결 |

여러 페이지에 걸친 블록은 첫 페이지에 나타난 segment만 bbox로 사용합니다. Semantic
payload에서 bbox가 없을 때는 `not_requested`, `no_provenance`, `unresolved`,
`not_applicable` 중 하나로 이유를 구분합니다.

---

## 커버리지

### DocLang으로 매핑되는 요소

| HWP 요소 | DocLang | 비고 |
|----------|---------|------|
| 문단 | `<text>` | |
| 개요 제목 | `<heading level="1–6">` | 글꼴 크기 추측이 아닌 개요 수준 기준 |
| 목록(번호/글머리표) | `<list>` / `<ldiv>` / `<marker>` | 중첩 지원 |
| 인라인 서식 | `<bold>` `<italic>` `<underline>` `<strikethrough>` `<superscript>` `<subscript>` | |
| 표 | OTSL (`<fcel>` `<ched>` `<lcel>` `<ucel>` `<xcel>` …) | 병합 · 중첩 셀 |
| 수식 | `<formula>` (LaTeX) | EqEdit → LaTeX; 변환 누락 토큰은 보고됨 |
| 이미지 | `<picture><src uri="…"/>` | 기본값은 인라인 base64; asset/URI 참조 정책 지원 |
| 각주 | `<footnote>` | |
| 머리말 / 꼬리말 | `<page_header>` / `<page_footer>` | |
| 쪽 나누기 | `<page_break/>` | |
| 다단 연속성 | `<thread>` | |
| 레이아웃 좌표 *(옵션)* | `<location>` / payload `location` | rhwp render tree 기반; 위 범위 참고 |

### 범위 외 (v1)

- **입력:** 암호화 문서(거부), 배포용 문서(`rhwp`는 파싱 가능하나 정책상 거부),
  HWP 3.x, 레거시 HWPML.
- **출력:** 역변환(DocLang → HWP); 렌더링(SVG/PDF — `rhwp`의 영역).
- **좌표:** 글상자 내부 콘텐츠, 각주/미주 본문, 셀 내부 객체에는 `<location>`을
  부착하지 않습니다(이들의 래퍼 표에는 부착됨). Payload에서는 좌표가 없는 이유를
  `not_requested`, `no_provenance`, `unresolved`, `not_applicable` 상태로 구분합니다.

### 손실 보고

`Lean` 모드에서 DocLang이 표현할 수 없는 정보는 모두 `LossEntry`(`kind`,
`location`, `detail`)로 기록됩니다. 종류에는 `FontInfo`, `CharColor`,
`NamedStyle`, `SectionSettings`, `FloatingObject`, `TextBox`, `TrackChanges`,
`FormulaFallback`, `Caption`, `Other`가 있습니다. 어떤 문서의 보고가 비어 있지
않다는 것은, 예를 들어 인제스천 파이프라인이 해당 파일을 조용히 누락시키는 대신
더 풍부한(예: VLM) 경로로 분기시킬 수 있다는 의미입니다.

DocLang 대응 요소가 없는 일부 컨트롤(HWP3 하이퍼링크 표시 텍스트, 덧말(Ruby),
글자겹침(CharOverlap))은 텍스트를 버리지 않고 평문 블록으로 **구제**하며, 잃은
의미(링크/덧말/겹침)는 `LossEntry`로 기록합니다. 의도적으로 v2로 미룬 변환 한계
(인라인 `<href>`, 인텍스트 객체 위치, 미주 별도 표현 등)는
[`docs/v2-known-limitations.md`](docs/v2-known-limitations.md)에 정리되어 있습니다.

---

## 아키텍처

```text
 HWP 5.0 (.hwp) ─┐
                 ├─► rhwp::parse_document ─► Hangulang Semantic IR ─┬─► DocLang XML
 HWPX (.hwpx) ──┘     (parser_adapter)      (rhwp 비의존)             ├─► semantic JSON payload
                                                                       ├─► Markdown
                                                                       ├─► resource assets / URIs
                                                                       └─► Python API payload (planned)
                                                     ▲
                             eqedit 패스 ────────────┘   (EqEdit script → LaTeX)
                             geometry 패스 ──────────┘   (렌더 트리 → location/bbox, 옵션)
```

Pandoc의 reader → IR → writer 패턴을 본뜬 3단계 파이프라인입니다. Semantic IR은
**rhwp 비의존**이며, `parser_adapter`만 `rhwp` 타입을 다룹니다 — upstream 변경으로
부터 변환기를 격리합니다.

| 모듈 | 역할 |
|------|------|
| `parser_adapter` | rhwp `Document` → Semantic IR (유일한 rhwp 의존 계층) |
| `ir` | rhwp 비의존 문서 모델(블록, 인라인, 표, 수식) |
| `eqedit` | HWP EqEdit 수식 스크립트 → LaTeX |
| `writer` | Semantic IR → DocLang v0.6 XML (OTSL 표, 모드, `<location>`) |
| `payload` | Semantic IR → stable payload / JSON schema |
| `markdown` | Semantic IR → Markdown |
| `resources` | 이미지 data URI, asset, URI prefix 정책 |
| `loss` | `LossReport` 수집 |

## 개발

```bash
cargo test                                     # 단위 + 골든 + 동등성 테스트
cargo test --features serde                    # JSON payload 직렬화 경로 포함
cargo test --features validator-integration    # 공식 doclang 검증기까지 실행
cargo clippy --all-targets                     # 린트 (경고 0 기대)
```

릴리즈 전 점검과 crates.io publish blocker는
[`docs/release.md`](docs/release.md)에 정리되어 있습니다.

### 검증기 연동

`validator-integration` 피처는 변환기 출력을 공식 Python 레퍼런스 검증기로
통과시킵니다. 최초 1회 설정:

```bash
python3 -m venv .venv
.venv/bin/pip install -r tests/requirements.txt   # doclang==0.6.0
```

venv가 없으면 검증기 테스트는 자동으로 건너뜁니다.

### 테스트 코퍼스

골든 파일 회귀 테스트는 실제 HWP/HWPX 문서를 대상으로 실행됩니다. fixture의
출처와 라이선스는
[`tests/fixtures/SOURCES.md`](tests/fixtures/SOURCES.md)에 문서화되어 있습니다.
의도적인 출력 변경 후에는 `UPDATE_GOLDEN=1 cargo test --test golden`으로 골든을
재생성하세요.

---

## 로드맵

- `hangulang-python`: Rust core를 감싼 Python wheel과 Pythonic API.
- Docling optional adapter 또는 plugin.
- LangChain/LlamaIndex document loader.
- EqEdit → LaTeX 심볼 커버리지 확대 (현재 누락 토큰은 보고되지만 아직 완전히
  변환되지는 않음).
- CLI의 JSON/Markdown 출력 옵션과 asset directory 쓰기.
- 글상자 / 각주 / 셀 내부 콘텐츠의 좌표 지원.
- SVG/PDF 빌드 비용을 제거하기 위한 `rhwp` upstream `parser-only` 피처.

---

## 라이선스

MIT. 자세한 내용은 [`LICENSE`](LICENSE)를 참고하세요.

본 프로젝트는 독립적인 오픈소스 프로젝트입니다. HWP/HWPX는 한글과컴퓨터(Hancom
Inc.)의 포맷이며, 본 프로젝트는 한컴과 제휴 관계가 없습니다. DocLang은 LF AI &
Data Foundation의 프로젝트입니다. `rhwp`는 © Edward Kim (MIT)입니다.
