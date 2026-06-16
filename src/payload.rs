//! Stable semantic payload exported from the Semantic IR.
//!
//! The payload is intentionally separate from the internal IR. It gives Python
//! wrappers and downstream integrations a versioned, serde-friendly contract
//! while allowing the internal lowering model to evolve.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::ir::block::{Block, HeaderFooterApply};
use crate::ir::inline::Inline;
use crate::ir::prov::{Location, LocationMap, Prov, LOCATION_RESOLUTION};
use crate::ir::style::StyleFlags;
use crate::ir::table::{Table, TableCell};
use crate::ir::SirDocument;
use crate::loss::{LossEntry, LossKind, LossReport};
use crate::options::ConvertOptions;
use crate::resources::{resolve_resource, ResourceAsset};

/// Schema version for JSON/serde semantic payloads.
pub const PAYLOAD_SCHEMA_VERSION: &str = "hangulang.semantic.v1";

/// Top-level semantic extraction payload.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPayload {
    pub schema_version: String,
    pub doclang_version: String,
    pub sections: Vec<PayloadSection>,
    pub assets: Vec<PayloadAsset>,
    pub losses: Vec<PayloadLoss>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadSection {
    pub index: usize,
    pub blocks: Vec<PayloadBlock>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadBlock {
    pub id: String,
    pub kind: String,
    pub text: Option<String>,
    pub inlines: Vec<PayloadInline>,
    pub level: Option<u8>,
    pub ordered: Option<bool>,
    pub location: PayloadLocation,
    pub children: Vec<PayloadBlock>,
    pub items: Vec<PayloadListItem>,
    pub table: Option<PayloadTable>,
    pub formula: Option<PayloadFormula>,
    pub resource: Option<PayloadResource>,
    pub thread_id: Option<String>,
    pub custom_namespace: Option<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadInline {
    pub kind: String,
    pub text: Option<String>,
    pub styles: Option<PayloadStyleFlags>,
    pub footnote_number: Option<usize>,
    /// Link target for `kind == "href"`; `None` for every other inline kind.
    pub uri: Option<String>,
    pub children: Vec<PayloadInline>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadStyleFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub superscript: bool,
    pub subscript: bool,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadListItem {
    pub index: usize,
    pub blocks: Vec<PayloadBlock>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadTable {
    pub rows: u16,
    pub cols: u16,
    pub caption: Option<String>,
    pub cells: Vec<PayloadTableCell>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct PayloadTableCell {
    pub row: u16,
    pub col: u16,
    pub row_span: u16,
    pub col_span: u16,
    pub is_header: bool,
    pub blocks: Vec<PayloadBlock>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadFormula {
    pub raw_eqedit: String,
    pub latex: Option<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadResource {
    pub uri: String,
    pub mime: String,
    pub asset_path: Option<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadAsset {
    pub path: String,
    pub mime: String,
    pub data_base64: String,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadLocation {
    pub status: String,
    pub page: Option<u32>,
    pub bbox: Option<PayloadBBox>,
    pub resolution: Option<u16>,
    pub reason: Option<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PayloadBBox {
    pub x_min: u16,
    pub y_min: u16,
    pub x_max: u16,
    pub y_max: u16,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadLoss {
    pub kind: String,
    pub location: String,
    pub detail: String,
}

/// Build a stable semantic payload from the internal IR.
pub fn build_payload(
    doc: &SirDocument,
    opts: &ConvertOptions,
    locs: &LocationMap,
    loss: &LossReport,
) -> SemanticPayload {
    let mut ctx = PayloadCtx {
        opts,
        locs,
        assets: Vec::new(),
    };
    let sections = doc
        .sections
        .iter()
        .enumerate()
        .map(|(si, section)| PayloadSection {
            index: si,
            blocks: section
                .blocks
                .iter()
                .enumerate()
                .map(|(bi, block)| {
                    let loc = format!("section[{si}]/block[{bi}]");
                    ctx.block_payload(block, &loc)
                })
                .collect(),
        })
        .collect();

    SemanticPayload {
        schema_version: PAYLOAD_SCHEMA_VERSION.to_string(),
        doclang_version: doc.doclang_version.to_string(),
        sections,
        assets: ctx.assets.into_iter().map(asset_payload).collect(),
        losses: loss.iter().map(loss_payload).collect(),
    }
}

struct PayloadCtx<'a> {
    opts: &'a ConvertOptions,
    locs: &'a LocationMap,
    assets: Vec<ResourceAsset>,
}

impl PayloadCtx<'_> {
    fn block_payload(&mut self, block: &Block, loc: &str) -> PayloadBlock {
        match block {
            Block::Paragraph { content, prov, .. } => self.text_block(
                loc,
                "paragraph",
                None,
                content,
                location_payload(self.opts, self.locs, *prov),
            ),
            Block::Heading {
                level,
                content,
                prov,
                ..
            } => {
                let mut block = self.text_block(
                    loc,
                    "heading",
                    Some(*level),
                    content,
                    location_payload(self.opts, self.locs, *prov),
                );
                block.level = Some((*level).clamp(1, 6));
                block
            }
            Block::List {
                ordered,
                items,
                prov,
                ..
            } => PayloadBlock {
                id: loc.to_string(),
                kind: "list".to_string(),
                text: Some(
                    items
                        .iter()
                        .flat_map(|item| item.content.iter().map(block_text))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                inlines: Vec::new(),
                level: None,
                ordered: Some(*ordered),
                location: location_payload(self.opts, self.locs, *prov),
                children: Vec::new(),
                items: items
                    .iter()
                    .enumerate()
                    .map(|(ii, item)| {
                        let item_loc = format!("{loc}/ldiv[{ii}]");
                        PayloadListItem {
                            index: ii,
                            blocks: self.blocks_payload(&item.content, &item_loc),
                        }
                    })
                    .collect(),
                table: None,
                formula: None,
                resource: None,
                thread_id: None,
                custom_namespace: None,
            },
            Block::Table(table) => self.table_block(table, loc),
            Block::Picture {
                data,
                extension,
                prov,
                ..
            } => {
                let resource = resolve_resource(&self.opts.resource_policy, loc, extension, data);
                let asset_path = resource.asset.as_ref().map(|asset| asset.path.clone());
                if let Some(asset) = resource.asset {
                    self.assets.push(asset);
                }
                PayloadBlock {
                    id: loc.to_string(),
                    kind: "picture".to_string(),
                    text: None,
                    inlines: Vec::new(),
                    level: None,
                    ordered: None,
                    location: location_payload(self.opts, self.locs, *prov),
                    children: Vec::new(),
                    items: Vec::new(),
                    table: None,
                    formula: None,
                    resource: Some(PayloadResource {
                        uri: resource.uri,
                        mime: resource.mime,
                        asset_path,
                    }),
                    thread_id: None,
                    custom_namespace: None,
                }
            }
            Block::Formula(formula) => PayloadBlock {
                id: loc.to_string(),
                kind: "formula".to_string(),
                text: Some(
                    formula
                        .latex
                        .clone()
                        .unwrap_or_else(|| formula.raw_eqedit.clone()),
                ),
                inlines: Vec::new(),
                level: None,
                ordered: None,
                location: location_payload(self.opts, self.locs, formula.prov),
                children: Vec::new(),
                items: Vec::new(),
                table: None,
                formula: Some(PayloadFormula {
                    raw_eqedit: formula.raw_eqedit.clone(),
                    latex: formula.latex.clone(),
                }),
                resource: None,
                thread_id: None,
                custom_namespace: None,
            },
            Block::PageBreak => self.empty_block(loc, "page_break", None),
            Block::Footnote {
                number,
                content,
                prov,
            } => {
                let mut block = self.container_block(
                    loc,
                    "footnote",
                    content,
                    location_payload(self.opts, self.locs, *prov),
                );
                block.text = Some(format!("[^{number}] {}", blocks_text(content)));
                block
            }
            Block::PageHeader {
                content,
                apply,
                prov,
            } => {
                let mut block = self.container_block(
                    loc,
                    "page_header",
                    content,
                    location_payload(self.opts, self.locs, *prov),
                );
                block.text = Some(format!("{}: {}", apply_text(apply), blocks_text(content)));
                block
            }
            Block::PageFooter {
                content,
                apply,
                prov,
            } => {
                let mut block = self.container_block(
                    loc,
                    "page_footer",
                    content,
                    location_payload(self.opts, self.locs, *prov),
                );
                block.text = Some(format!("{}: {}", apply_text(apply), blocks_text(content)));
                block
            }
            Block::ThreadStart { thread_id } => {
                self.empty_block(loc, "thread_start", Some(thread_id.clone()))
            }
            Block::ThreadContinuation { thread_id } => {
                self.empty_block(loc, "thread_continuation", Some(thread_id.clone()))
            }
            Block::Custom { namespace, .. } => PayloadBlock {
                id: loc.to_string(),
                kind: "custom".to_string(),
                text: None,
                inlines: Vec::new(),
                level: None,
                ordered: None,
                location: PayloadLocation::not_applicable("custom block has no layout provenance"),
                children: Vec::new(),
                items: Vec::new(),
                table: None,
                formula: None,
                resource: None,
                thread_id: None,
                custom_namespace: Some(namespace.clone()),
            },
        }
    }

    fn text_block(
        &mut self,
        loc: &str,
        kind: &str,
        level: Option<u8>,
        content: &[Inline],
        location: PayloadLocation,
    ) -> PayloadBlock {
        PayloadBlock {
            id: loc.to_string(),
            kind: kind.to_string(),
            text: Some(inline_text(content)),
            inlines: content.iter().map(inline_payload).collect(),
            level,
            ordered: None,
            location,
            children: Vec::new(),
            items: Vec::new(),
            table: None,
            formula: None,
            resource: None,
            thread_id: None,
            custom_namespace: None,
        }
    }

    fn table_block(&mut self, table: &Table, loc: &str) -> PayloadBlock {
        PayloadBlock {
            id: loc.to_string(),
            kind: "table".to_string(),
            text: Some(blocks_text(
                &table
                    .cells
                    .iter()
                    .flat_map(|cell| cell.content.clone())
                    .collect::<Vec<_>>(),
            )),
            inlines: Vec::new(),
            level: None,
            ordered: None,
            location: location_payload(self.opts, self.locs, table.prov),
            children: Vec::new(),
            items: Vec::new(),
            table: Some(PayloadTable {
                rows: table.rows,
                cols: table.cols,
                caption: table.caption.as_ref().map(|c| inline_text(c)),
                cells: table
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(ci, cell)| {
                        let cell_loc = format!("{loc}/cell[{ci}]");
                        self.table_cell_payload(cell, &cell_loc)
                    })
                    .collect(),
            }),
            formula: None,
            resource: None,
            thread_id: None,
            custom_namespace: None,
        }
    }

    fn table_cell_payload(&mut self, cell: &TableCell, loc: &str) -> PayloadTableCell {
        PayloadTableCell {
            row: cell.row,
            col: cell.col,
            row_span: cell.row_span,
            col_span: cell.col_span,
            is_header: cell.is_header,
            blocks: self.blocks_payload(&cell.content, loc),
        }
    }

    fn container_block(
        &mut self,
        loc: &str,
        kind: &str,
        content: &[Block],
        location: PayloadLocation,
    ) -> PayloadBlock {
        PayloadBlock {
            id: loc.to_string(),
            kind: kind.to_string(),
            text: Some(blocks_text(content)),
            inlines: Vec::new(),
            level: None,
            ordered: None,
            location,
            children: self.blocks_payload(content, loc),
            items: Vec::new(),
            table: None,
            formula: None,
            resource: None,
            thread_id: None,
            custom_namespace: None,
        }
    }

    fn empty_block(&self, loc: &str, kind: &str, thread_id: Option<String>) -> PayloadBlock {
        PayloadBlock {
            id: loc.to_string(),
            kind: kind.to_string(),
            text: None,
            inlines: Vec::new(),
            level: None,
            ordered: None,
            location: PayloadLocation::not_applicable("block type has no layout payload"),
            children: Vec::new(),
            items: Vec::new(),
            table: None,
            formula: None,
            resource: None,
            thread_id,
            custom_namespace: None,
        }
    }

    fn blocks_payload(&mut self, blocks: &[Block], loc: &str) -> Vec<PayloadBlock> {
        blocks
            .iter()
            .enumerate()
            .map(|(bi, block)| {
                let child_loc = format!("{loc}/block[{bi}]");
                self.block_payload(block, &child_loc)
            })
            .collect()
    }
}

impl PayloadLocation {
    fn exact(loc: &Location) -> Self {
        PayloadLocation {
            status: "exact".to_string(),
            page: Some(loc.page),
            bbox: Some(PayloadBBox {
                x_min: loc.x_min,
                y_min: loc.y_min,
                x_max: loc.x_max,
                y_max: loc.y_max,
            }),
            resolution: Some(LOCATION_RESOLUTION),
            reason: None,
        }
    }

    fn fallback(status: &str, reason: &str) -> Self {
        PayloadLocation {
            status: status.to_string(),
            page: None,
            bbox: None,
            resolution: Some(LOCATION_RESOLUTION),
            reason: Some(reason.to_string()),
        }
    }

    fn not_applicable(reason: &str) -> Self {
        PayloadLocation {
            status: "not_applicable".to_string(),
            page: None,
            bbox: None,
            resolution: None,
            reason: Some(reason.to_string()),
        }
    }
}

fn location_payload(
    opts: &ConvertOptions,
    locs: &LocationMap,
    prov: Option<Prov>,
) -> PayloadLocation {
    if !opts.with_location {
        return PayloadLocation::fallback(
            "not_requested",
            "ConvertOptions::with_location is false",
        );
    }
    let Some(prov) = prov else {
        return PayloadLocation::fallback(
            "no_provenance",
            "IR block has no stable model provenance for layout join",
        );
    };
    locs.get(&prov)
        .map(PayloadLocation::exact)
        .unwrap_or_else(|| {
            PayloadLocation::fallback(
                "unresolved",
                "layout pass did not resolve a bounding box for this provenance",
            )
        })
}

fn inline_payload(inline: &Inline) -> PayloadInline {
    match inline {
        Inline::Text(text) => PayloadInline {
            kind: "text".to_string(),
            text: Some(text.clone()),
            styles: None,
            footnote_number: None,
            uri: None,
            children: Vec::new(),
        },
        Inline::Styled(flags, children) => PayloadInline {
            kind: "styled".to_string(),
            text: Some(inline_text(children)),
            styles: Some(style_payload(*flags)),
            footnote_number: None,
            uri: None,
            children: children.iter().map(inline_payload).collect(),
        },
        Inline::FootnoteRef(number) => PayloadInline {
            kind: "footnote_ref".to_string(),
            text: None,
            styles: None,
            footnote_number: Some(*number),
            uri: None,
            children: Vec::new(),
        },
        Inline::LineBreak => PayloadInline {
            kind: "line_break".to_string(),
            text: Some("\n".to_string()),
            styles: None,
            footnote_number: None,
            uri: None,
            children: Vec::new(),
        },
        Inline::Tab => PayloadInline {
            kind: "tab".to_string(),
            text: Some("\t".to_string()),
            styles: None,
            footnote_number: None,
            uri: None,
            children: Vec::new(),
        },
        Inline::Href { uri, content } => PayloadInline {
            kind: "href".to_string(),
            text: Some(inline_text(content)),
            styles: None,
            footnote_number: None,
            uri: Some(uri.clone()),
            children: content.iter().map(inline_payload).collect(),
        },
    }
}

fn style_payload(flags: StyleFlags) -> PayloadStyleFlags {
    PayloadStyleFlags {
        bold: flags.bold,
        italic: flags.italic,
        underline: flags.underline,
        strike: flags.strike,
        superscript: flags.superscript,
        subscript: flags.subscript,
    }
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Styled(_, children) => out.push_str(&inline_text(children)),
            Inline::FootnoteRef(number) => out.push_str(&format!("[^{number}]")),
            Inline::LineBreak => out.push('\n'),
            Inline::Tab => out.push('\t'),
            Inline::Href { content, .. } => out.push_str(&inline_text(content)),
        }
    }
    out
}

fn block_text(block: &Block) -> String {
    match block {
        Block::Paragraph { content, .. } | Block::Heading { content, .. } => inline_text(content),
        Block::List { items, .. } => items
            .iter()
            .map(|item| blocks_text(&item.content))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Table(table) => table
            .cells
            .iter()
            .map(|cell| blocks_text(&cell.content))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Picture { .. } => String::new(),
        Block::Formula(formula) => formula
            .latex
            .clone()
            .unwrap_or_else(|| formula.raw_eqedit.clone()),
        Block::PageBreak => String::new(),
        Block::Footnote { content, .. }
        | Block::PageHeader { content, .. }
        | Block::PageFooter { content, .. } => blocks_text(content),
        Block::ThreadStart { .. } | Block::ThreadContinuation { .. } | Block::Custom { .. } => {
            String::new()
        }
    }
}

fn blocks_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(block_text)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_text(apply: &HeaderFooterApply) -> &'static str {
    match apply {
        HeaderFooterApply::All => "all",
        HeaderFooterApply::Even => "even",
        HeaderFooterApply::Odd => "odd",
        HeaderFooterApply::First => "first",
    }
}

fn asset_payload(asset: ResourceAsset) -> PayloadAsset {
    PayloadAsset {
        path: asset.path,
        mime: asset.mime,
        data_base64: BASE64.encode(asset.data),
    }
}

fn loss_payload(loss: &LossEntry) -> PayloadLoss {
    PayloadLoss {
        kind: loss_kind_name(&loss.kind),
        location: loss.location.clone(),
        detail: loss.detail.clone(),
    }
}

fn loss_kind_name(kind: &LossKind) -> String {
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
        LossKind::Other(value) => format!("other:{value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::block::Block;
    use crate::ir::inline::Inline;
    use crate::ir::prov::{Location, Prov};
    use crate::ir::{Section, SirDocument};
    use crate::resources::ResourcePolicy;

    fn doc(blocks: Vec<Block>) -> SirDocument {
        SirDocument {
            sections: vec![Section { blocks }],
            doclang_version: "0.6",
        }
    }

    #[test]
    fn location_status_is_not_requested_by_default() {
        let payload = build_payload(
            &doc(vec![Block::Paragraph {
                content: vec![Inline::Text("hello".into())],
                lost: None,
                prov: Some(Prov::text(0, 0)),
            }]),
            &ConvertOptions::default(),
            &LocationMap::new(),
            &LossReport::new(),
        );
        let block = &payload.sections[0].blocks[0];
        assert_eq!(block.kind, "paragraph");
        assert_eq!(block.text.as_deref(), Some("hello"));
        assert_eq!(block.location.status, "not_requested");
    }

    #[test]
    fn exact_location_carries_page_and_bbox() {
        let mut locs = LocationMap::new();
        locs.insert(
            Prov::text(0, 0),
            Location {
                page: 2,
                x_min: 1,
                y_min: 2,
                x_max: 3,
                y_max: 4,
            },
        );
        let opts = ConvertOptions {
            with_location: true,
            ..ConvertOptions::default()
        };
        let payload = build_payload(
            &doc(vec![Block::Paragraph {
                content: vec![Inline::Text("hello".into())],
                lost: None,
                prov: Some(Prov::text(0, 0)),
            }]),
            &opts,
            &locs,
            &LossReport::new(),
        );
        let location = &payload.sections[0].blocks[0].location;
        assert_eq!(location.status, "exact");
        assert_eq!(location.page, Some(2));
        assert_eq!(location.bbox.unwrap().x_max, 3);
    }

    #[test]
    fn asset_dir_policy_adds_payload_asset() {
        let opts = ConvertOptions {
            resource_policy: ResourcePolicy::asset_dir("assets"),
            ..ConvertOptions::default()
        };
        let payload = build_payload(
            &doc(vec![Block::Picture {
                data: b"abc".to_vec(),
                extension: "png".into(),
                geometry: None,
                lost: None,
                prov: None,
            }]),
            &opts,
            &LocationMap::new(),
            &LossReport::new(),
        );
        let block = &payload.sections[0].blocks[0];
        assert_eq!(
            block.resource.as_ref().unwrap().uri,
            "assets/section-0-block-0.png"
        );
        assert_eq!(payload.assets.len(), 1);
        assert_eq!(payload.assets[0].path, "section-0-block-0.png");
        assert_eq!(payload.assets[0].data_base64, "YWJj");
    }
}
