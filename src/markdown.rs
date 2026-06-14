//! Direct Markdown exporter for the Semantic IR.
//!
//! This writer does not round-trip through DocLang XML. It consumes the same
//! rhwp-agnostic IR as the XML writer and keeps resource policy handling shared
//! with the rest of the crate.

use crate::ir::block::{Block, HeaderFooterApply, ListItem};
use crate::ir::formula::Formula;
use crate::ir::inline::Inline;
use crate::ir::style::StyleFlags;
use crate::ir::table::Table;
use crate::ir::SirDocument;
use crate::loss::{LossEntry, LossKind, LossReport};
use crate::options::ConvertOptions;
use crate::resources::{resolve_resource, ResourceAsset};

/// Markdown plus any assets that must be written by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownExport {
    /// Markdown document text.
    pub markdown: String,
    /// Binary assets referenced from the Markdown.
    pub assets: Vec<ResourceAsset>,
}

/// Write a Semantic IR document directly to Markdown.
pub fn write_markdown(
    doc: &SirDocument,
    opts: &ConvertOptions,
    loss: &mut LossReport,
) -> MarkdownExport {
    let mut ctx = MarkdownCtx {
        out: String::new(),
        assets: Vec::new(),
        opts,
        loss,
    };
    for (si, section) in doc.sections.iter().enumerate() {
        for (bi, block) in section.blocks.iter().enumerate() {
            let loc = format!("section[{si}]/block[{bi}]");
            ctx.write_block(block, &loc, 0);
        }
    }
    MarkdownExport {
        markdown: trim_trailing_blank_lines(ctx.out),
        assets: ctx.assets,
    }
}

struct MarkdownCtx<'a> {
    out: String,
    assets: Vec<ResourceAsset>,
    opts: &'a ConvertOptions,
    loss: &'a mut LossReport,
}

impl MarkdownCtx<'_> {
    fn write_block(&mut self, block: &Block, loc: &str, depth: usize) {
        match block {
            Block::Paragraph { content, .. } => {
                self.push_line(&inline_markdown(content));
                self.blank();
            }
            Block::Heading { level, content, .. } => {
                let level = (*level).clamp(1, 6) as usize;
                self.push_line(&format!(
                    "{} {}",
                    "#".repeat(level),
                    inline_markdown(content)
                ));
                self.blank();
            }
            Block::List { ordered, items, .. } => {
                for (ii, item) in items.iter().enumerate() {
                    let item_loc = format!("{loc}/ldiv[{ii}]");
                    self.write_list_item(*ordered, item, &item_loc, depth);
                }
                self.blank();
            }
            Block::Table(table) => {
                self.write_table(table);
                self.blank();
            }
            Block::Picture {
                data, extension, ..
            } => {
                let resource = resolve_resource(&self.opts.resource_policy, loc, extension, data);
                if let Some(asset) = resource.asset {
                    self.assets.push(asset);
                }
                self.push_line(&format!("![image]({})", escape_link_dest(&resource.uri)));
                self.blank();
            }
            Block::Formula(formula) => {
                self.write_formula(formula, loc);
                self.blank();
            }
            Block::PageBreak => {
                self.push_line("\\pagebreak");
                self.blank();
            }
            Block::Footnote {
                number, content, ..
            } => {
                let text = plain_text_blocks(content);
                self.push_line(&format!("[^{number}]: {}", text.trim()));
                self.blank();
            }
            Block::PageHeader { content, apply, .. } => {
                self.record_apply_loss(apply, loc, "page_header");
                self.write_nested_blocks(content, loc, depth);
            }
            Block::PageFooter { content, apply, .. } => {
                self.record_apply_loss(apply, loc, "page_footer");
                self.write_nested_blocks(content, loc, depth);
            }
            Block::ThreadStart { .. } | Block::ThreadContinuation { .. } => {}
            Block::Custom { namespace, .. } => {
                self.loss.push(LossEntry {
                    kind: LossKind::FloatingObject,
                    location: loc.to_string(),
                    detail: format!("custom block omitted from Markdown (ns={namespace})"),
                });
            }
        }
    }

    fn write_nested_blocks(&mut self, blocks: &[Block], loc: &str, depth: usize) {
        for (bi, block) in blocks.iter().enumerate() {
            let child_loc = format!("{loc}/block[{bi}]");
            self.write_block(block, &child_loc, depth);
        }
    }

    fn write_list_item(&mut self, ordered: bool, item: &ListItem, loc: &str, depth: usize) {
        let marker = if ordered { "1." } else { "-" };
        let indent = "  ".repeat(depth);
        let first = item.content.first().map(block_summary).unwrap_or_default();
        self.push_line(&format!("{indent}{marker} {first}"));
        for (bi, block) in item.content.iter().enumerate().skip(1) {
            let child_loc = format!("{loc}/block[{bi}]");
            self.write_block(block, &child_loc, depth + 1);
        }
    }

    fn write_table(&mut self, table: &Table) {
        if let Some(caption) = &table.caption {
            self.push_line(&format!("_{}_", inline_markdown(caption)));
            self.blank();
        }

        let rows = table.rows as usize;
        let cols = table.cols as usize;
        if rows == 0 || cols == 0 {
            return;
        }

        let mut grid = vec![vec![String::new(); cols]; rows];
        for cell in &table.cells {
            let row = cell.row as usize;
            let col = cell.col as usize;
            if row < rows && col < cols {
                grid[row][col] = plain_text_blocks(&cell.content);
            }
        }

        self.push_line(&markdown_table_row(&grid[0]));
        self.push_line(&markdown_table_separator(cols));
        for row in grid.iter().skip(1) {
            self.push_line(&markdown_table_row(row));
        }
    }

    fn write_formula(&mut self, formula: &Formula, loc: &str) {
        match &formula.latex {
            Some(latex) => {
                self.push_line("$$");
                self.push_line(latex);
                self.push_line("$$");
            }
            None => {
                self.push_line("```eqedit");
                self.push_line(&formula.raw_eqedit);
                self.push_line("```");
                self.loss.push(LossEntry {
                    kind: LossKind::FormulaFallback,
                    location: loc.to_string(),
                    detail: format!(
                        "no LaTeX conversion; emitted raw EqEdit script in Markdown: {}",
                        formula.raw_eqedit
                    ),
                });
            }
        }
    }

    fn record_apply_loss(&mut self, apply: &HeaderFooterApply, loc: &str, element: &str) {
        let scope = match apply {
            HeaderFooterApply::All => return,
            HeaderFooterApply::Even => "even pages",
            HeaderFooterApply::Odd => "odd pages",
            HeaderFooterApply::First => "first page",
        };
        self.loss.push(LossEntry {
            kind: LossKind::SectionSettings,
            location: loc.to_string(),
            detail: format!("{element} applies to {scope}; Markdown has no page-scope marker"),
        });
    }

    fn push_line(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn blank(&mut self) {
        if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }
}

fn inline_markdown(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        write_inline(&mut out, inline);
    }
    out
}

fn write_inline(out: &mut String, inline: &Inline) {
    match inline {
        Inline::Text(text) => out.push_str(&escape_markdown_text(text)),
        Inline::Styled(flags, children) => write_styled(out, flags, children),
        Inline::FootnoteRef(number) => out.push_str(&format!("[^{number}]")),
        Inline::LineBreak => out.push('\n'),
        Inline::Tab => out.push(' '),
    }
}

fn write_styled(out: &mut String, flags: &StyleFlags, children: &[Inline]) {
    let body = inline_markdown(children);
    let mut text = body;
    if flags.subscript {
        text = format!("<sub>{text}</sub>");
    }
    if flags.superscript {
        text = format!("<sup>{text}</sup>");
    }
    if flags.strike {
        text = format!("~~{text}~~");
    }
    if flags.italic {
        text = format!("*{text}*");
    }
    if flags.bold {
        text = format!("**{text}**");
    }
    out.push_str(&text);
}

fn block_summary(block: &Block) -> String {
    match block {
        Block::Paragraph { content, .. } | Block::Heading { content, .. } => {
            inline_markdown(content)
        }
        Block::Formula(formula) => formula
            .latex
            .clone()
            .unwrap_or_else(|| formula.raw_eqedit.clone()),
        Block::Picture { .. } => "[image]".to_string(),
        Block::PageBreak => "\\pagebreak".to_string(),
        Block::Table(_) => "[table]".to_string(),
        Block::Footnote { number, .. } => format!("[^{number}]"),
        Block::List { items, .. } => items
            .iter()
            .map(|i| plain_text_blocks(&i.content))
            .collect::<Vec<_>>()
            .join(" "),
        Block::PageHeader { content, .. } | Block::PageFooter { content, .. } => {
            plain_text_blocks(content)
        }
        Block::ThreadStart { .. } | Block::ThreadContinuation { .. } | Block::Custom { .. } => {
            String::new()
        }
    }
}

fn plain_text_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(block_summary)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn markdown_table_row(cells: &[String]) -> String {
    let body = cells
        .iter()
        .map(|cell| cell.replace('|', "\\|").replace('\n', " "))
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {body} |")
}

fn markdown_table_separator(cols: usize) -> String {
    let body = std::iter::repeat_n("---", cols)
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {body} |")
}

fn escape_markdown_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '*' | '_' | '[' | ']' | '`' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn escape_link_dest(uri: &str) -> String {
    uri.replace(')', "%29").replace(' ', "%20")
}

fn trim_trailing_blank_lines(mut text: String) -> String {
    while text.ends_with('\n') {
        text.pop();
    }
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::block::Block;
    use crate::ir::inline::Inline;
    use crate::ir::style::StyleFlags;
    use crate::ir::{Section, SirDocument};
    use crate::resources::ResourcePolicy;

    fn doc(blocks: Vec<Block>) -> SirDocument {
        SirDocument {
            sections: vec![Section { blocks }],
            doclang_version: "0.6",
        }
    }

    #[test]
    fn writes_headings_paragraphs_and_styles() {
        let bold = StyleFlags {
            bold: true,
            ..Default::default()
        };
        let mut loss = LossReport::new();
        let out = write_markdown(
            &doc(vec![
                Block::Heading {
                    level: 2,
                    content: vec![Inline::Text("Title".into())],
                    lost: None,
                    prov: None,
                },
                Block::Paragraph {
                    content: vec![Inline::Styled(bold, vec![Inline::Text("body".into())])],
                    lost: None,
                    prov: None,
                },
            ]),
            &ConvertOptions::default(),
            &mut loss,
        );
        assert_eq!(out.markdown, "## Title\n\n**body**\n");
        assert!(loss.is_empty());
    }

    #[test]
    fn asset_policy_returns_markdown_assets() {
        let mut loss = LossReport::new();
        let opts = ConvertOptions {
            resource_policy: ResourcePolicy::asset_dir("assets"),
            ..ConvertOptions::default()
        };
        let out = write_markdown(
            &doc(vec![Block::Picture {
                data: b"abc".to_vec(),
                extension: "png".into(),
                geometry: None,
                lost: None,
                prov: None,
            }]),
            &opts,
            &mut loss,
        );
        assert_eq!(out.markdown, "![image](assets/section-0-block-0.png)\n");
        assert_eq!(out.assets.len(), 1);
        assert_eq!(out.assets[0].path, "section-0-block-0.png");
    }

    #[test]
    fn writes_simple_table() {
        let mut loss = LossReport::new();
        let table = Table {
            rows: 2,
            cols: 2,
            caption: None,
            prov: None,
            cells: vec![
                crate::ir::TableCell {
                    row: 0,
                    col: 0,
                    row_span: 1,
                    col_span: 1,
                    is_header: false,
                    content: vec![Block::Paragraph {
                        content: vec![Inline::Text("A".into())],
                        lost: None,
                        prov: None,
                    }],
                },
                crate::ir::TableCell {
                    row: 0,
                    col: 1,
                    row_span: 1,
                    col_span: 1,
                    is_header: false,
                    content: vec![Block::Paragraph {
                        content: vec![Inline::Text("B".into())],
                        lost: None,
                        prov: None,
                    }],
                },
            ],
        };
        let out = write_markdown(
            &doc(vec![Block::Table(table)]),
            &ConvertOptions::default(),
            &mut loss,
        );
        assert!(out.markdown.contains("| A | B |"));
        assert!(out.markdown.contains("| --- | --- |"));
    }
}
