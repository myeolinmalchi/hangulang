//! v2 bbox probe: verifies that rhwp's PageLayerTree nodes can be mapped back
//! to the document-model elements our Semantic IR is built from.
//!
//! Resolution rules under test (from rhwp render_tree.rs docs + empirical runs):
//!   - top-level TextLine: (section_index, para_index) index into
//!     Document.sections[s].paragraphs[p]
//!   - content inside a table cell uses CELL-LOCAL indices; the cell is
//!     identified by the ancestor chain Table -> TableCell(model_cell_index)
//!   - nested tables resolve against the enclosing cell's paragraphs
//!   - header/footer/footnote-area/master-page/textbox content owns its own
//!     paragraph space (needs source_node_id for provenance; counted, not failed)
//!   - para_index == usize::MAX is a sentinel for synthetic/furniture lines
//!
//! Usage: cargo run --example layer_probe -- <input.hwp>

use rhwp::model::control::Control;
use rhwp::model::document::Document;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::table::{Cell, Table};
use rhwp::paint::layer_tree::{GroupKind, LayerNode, LayerNodeKind};
use rhwp::DocumentCore;

#[derive(Default)]
struct Stats {
    body_lines: usize,
    body_lines_resolved: usize,
    cell_lines: usize,
    cell_lines_resolved: usize,
    tables: usize,
    tables_identity_ok: usize,
    nested_tables_ok: usize,
    cells: usize,
    cells_resolved: usize,
    cells_pos_exact: usize,
    cells_pos_rebased: usize,
    local_tables: usize,
    local_cells: usize,
    local_lines: usize,
    sentinel_lines: usize,
    unresolved_tables: usize,
    samples: Vec<String>,
    failures: Vec<String>,
}

fn table_in(paras: &[Paragraph], p: usize, c: usize) -> Option<&Table> {
    match paras.get(p)?.controls.get(c)? {
        Control::Table(t) => Some(t),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct Ctx<'a> {
    table: Option<&'a Table>,
    cell: Option<&'a Cell>,
    local: bool, // header/footer/footnote/master-page/textbox subtree
}

fn walk<'a>(node: &LayerNode, page: u32, doc: &'a Document, ctx: Ctx<'a>, stats: &mut Stats) {
    let mut next = ctx;

    if let LayerNodeKind::Group { group_kind, .. } = &node.kind {
        match group_kind {
            GroupKind::TextBox
            | GroupKind::Header
            | GroupKind::Footer
            | GroupKind::FootnoteArea
            | GroupKind::MasterPage => next.local = true,

            GroupKind::Table(t) => {
                if ctx.local {
                    stats.local_tables += 1;
                    next.table = None;
                    next.cell = None;
                } else {
                    stats.tables += 1;
                    let (s, p, c) = match (t.section_index, t.para_index, t.control_index) {
                        (Some(s), Some(p), Some(c)) => (s, p, c),
                        _ => {
                            stats.unresolved_tables += 1;
                            next.local = true;
                            next.table = None;
                            next.cell = None;
                            return walk_children(node, page, doc, next, stats);
                        }
                    };
                    // nested table → resolve in enclosing cell's paragraph space;
                    // top-level → document-global space
                    let model = match ctx.cell {
                        Some(cell) => table_in(&cell.paragraphs, p, c),
                        None => doc.sections.get(s).and_then(|sec| table_in(&sec.paragraphs, p, c)),
                    };
                    match model {
                        Some(m) if m.row_count == t.row_count && m.col_count == t.col_count => {
                            stats.tables_identity_ok += 1;
                            if ctx.cell.is_some() {
                                stats.nested_tables_ok += 1;
                            }
                            if stats.samples.len() < 8 {
                                stats.samples.push(format!(
                                    "page {page} Table{} ({s},{p},{c}) {}x{} bounds=({:.0},{:.0} {:.0}x{:.0})",
                                    if ctx.cell.is_some() { "[nested]" } else { "" },
                                    m.row_count, m.col_count,
                                    node.bounds.x, node.bounds.y, node.bounds.width, node.bounds.height
                                ));
                            }
                            next.table = Some(m);
                            next.cell = None;
                        }
                        Some(m) => {
                            stats.failures.push(format!(
                                "page {page} Table ({s},{p},{c}): dims mismatch layer {}x{} vs model {}x{}",
                                t.row_count, t.col_count, m.row_count, m.col_count
                            ));
                            next.local = true;
                            next.table = None;
                            next.cell = None;
                        }
                        None => {
                            stats.unresolved_tables += 1;
                            next.local = true;
                            next.table = None;
                            next.cell = None;
                        }
                    }
                }
            }

            GroupKind::TableCell(c) => {
                if next.local && next.table.is_none() {
                    stats.local_cells += 1;
                } else {
                    stats.cells += 1;
                    match (next.table, c.model_cell_index) {
                        (Some(table), Some(idx)) => match table.cells.get(idx as usize) {
                            Some(cell) => {
                                stats.cells_resolved += 1;
                                if cell.col == c.col && cell.row == c.row {
                                    stats.cells_pos_exact += 1;
                                } else {
                                    stats.cells_pos_rebased += 1;
                                }
                                next.cell = Some(cell);
                            }
                            None => stats.failures.push(format!(
                                "page {page} Cell model_idx={idx} (r{},c{}): index out of model range",
                                c.row, c.col
                            )),
                        },
                        _ => stats.failures.push(format!(
                            "page {page} Cell (r{},c{}): no table context or no model_cell_index",
                            c.row, c.col
                        )),
                    }
                }
            }

            GroupKind::TextLine(t) => {
                if let (Some(s), Some(p)) = (t.section_index, t.para_index) {
                    if p == usize::MAX || s == usize::MAX {
                        stats.sentinel_lines += 1;
                    } else if let Some(cell) = next.cell {
                        stats.cell_lines += 1;
                        if p < cell.paragraphs.len() {
                            stats.cell_lines_resolved += 1;
                        } else {
                            stats.failures.push(format!(
                                "page {page} cell TextLine para={p}: cell-local para out of range"
                            ));
                        }
                    } else if next.local {
                        stats.local_lines += 1;
                    } else {
                        stats.body_lines += 1;
                        if doc.sections.get(s).map(|sec| p < sec.paragraphs.len()).unwrap_or(false) {
                            stats.body_lines_resolved += 1;
                        } else {
                            stats.failures.push(format!(
                                "page {page} body TextLine sec={s} para={p}: out of range"
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    walk_children(node, page, doc, next, stats);
}

fn walk_children<'a>(node: &LayerNode, page: u32, doc: &'a Document, ctx: Ctx<'a>, stats: &mut Stats) {
    match &node.kind {
        LayerNodeKind::Group { children, .. } => {
            for child in children {
                walk(child, page, doc, ctx, stats);
            }
        }
        LayerNodeKind::ClipRect { child, .. } => walk(child, page, doc, ctx, stats),
        LayerNodeKind::Leaf { .. } => {}
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: layer_probe <input.hwp>");
    let data = std::fs::read(&path).expect("read input");

    let doc = rhwp::parse_document(&data).expect("parse_document");
    let core = DocumentCore::from_bytes(&data).expect("DocumentCore::from_bytes");
    let pages = core.page_count();

    let mut stats = Stats::default();
    for page in 0..pages {
        let tree = core
            .build_page_layer_tree(page)
            .unwrap_or_else(|e| panic!("layer tree page {page}: {e:?}"));
        walk(&tree.root, page, &doc, Ctx { table: None, cell: None, local: false }, &mut stats);
    }

    println!("file: {path} | pages: {pages} | sections: {}", doc.sections.len());
    println!();
    println!("Tables:     {}/{} identity-verified (incl. {} nested via cell-local resolution)", stats.tables_identity_ok, stats.tables, stats.nested_tables_ok);
    println!("TableCells: {}/{} resolved via model_cell_index (pos exact: {}, rebased by page split: {})", stats.cells_resolved, stats.cells, stats.cells_pos_exact, stats.cells_pos_rebased);
    println!("Body lines: {}/{} resolved to sections[s].paragraphs[p]", stats.body_lines_resolved, stats.body_lines);
    println!("Cell lines: {}/{} resolved cell-locally via ancestor chain", stats.cell_lines_resolved, stats.cell_lines);
    println!("Local-context (header/footer/footnote-area/master-page/textbox — needs source_node_id): tables={} cells={} lines={}", stats.local_tables, stats.local_cells, stats.local_lines);
    println!("Sentinel lines (usize::MAX — synthetic/furniture): {}", stats.sentinel_lines);
    println!("Tables unresolved (no indices / not found in expected space): {}", stats.unresolved_tables);
    println!();
    if stats.failures.is_empty() {
        println!("FAILURES: none — mapping rule holds for every node");
    } else {
        println!("FAILURES ({}):", stats.failures.len());
        for f in stats.failures.iter().take(10) {
            println!("  {f}");
        }
    }
    println!();
    println!("samples:");
    for s in stats.samples.iter().take(8) {
        println!("  {s}");
    }
}
