//! Follow-up probes for the 3 gaps left by layer_probe:
//!   gap1: can local-context layer nodes (header/footer/textbox/master-page)
//!         be resolved via source_node_id -> render tree provenance?
//!   gap2: do out-of-range body line para indices correspond to endnote
//!         content paragraphs appended after the body?
//!   gap3: what is the model reality behind the exam-social dims mismatch
//!         Table (0,15,0) layer 6x3 vs model 1x1?
//!
//! Usage: cargo run --example gap_probe -- <gap1|gap2|gap3> <input.hwp>

use std::collections::HashMap;

use rhwp::model::control::Control;
use rhwp::model::document::Document;
use rhwp::paint::layer_tree::{GroupKind, LayerNode, LayerNodeKind};
use rhwp::renderer::render_tree::{NodeId, RenderNode, RenderNodeType};
use rhwp::DocumentCore;

fn provenance_of(n: &RenderNodeType) -> Option<(usize, usize, Option<usize>)> {
    match n {
        RenderNodeType::TextLine(t) => match (t.section_index, t.para_index) {
            (Some(s), Some(p)) => Some((s, p, None)),
            _ => None,
        },
        RenderNodeType::Table(t) => match (t.section_index, t.para_index, t.control_index) {
            (Some(s), Some(p), c) => Some((s, p, c)),
            _ => None,
        },
        RenderNodeType::Line(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Rectangle(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Ellipse(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Path(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Image(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Equation(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::Group(x) => zip3(x.section_index, x.para_index, x.control_index),
        RenderNodeType::FormObject(x) => Some((x.section_index, x.para_index, Some(x.control_index))),
        _ => None,
    }
}

fn zip3(s: Option<usize>, p: Option<usize>, c: Option<usize>) -> Option<(usize, usize, Option<usize>)> {
    match (s, p) {
        (Some(s), Some(p)) => Some((s, p, c)),
        _ => None,
    }
}

fn index_render<'a>(n: &'a RenderNode, map: &mut HashMap<NodeId, &'a RenderNode>) {
    map.insert(n.id, n);
    for ch in &n.children {
        index_render(ch, map);
    }
}

/// first provenance found at node or any descendant (pre-order)
fn find_provenance(n: &RenderNode) -> Option<(usize, usize, Option<usize>)> {
    if let Some(p) = provenance_of(&n.node_type) {
        return Some(p);
    }
    n.children.iter().find_map(find_provenance)
}

// ---------- gap1 ----------

#[derive(Default)]
struct Gap1 {
    local_nodes: usize,
    with_source_id: usize,
    id_in_render: usize,
    provenance_found: usize,
    provenance_valid: usize,
    // local-context ROOT groups (TextBox/Header/Footer/FootnoteArea/MasterPage boundary nodes)
    roots: usize,
    roots_resolved: usize,
    root_kinds_unresolved: Vec<String>,
}

fn gap1_walk(node: &LayerNode, doc: &Document, render: &HashMap<NodeId, &RenderNode>, in_local: bool, g: &mut Gap1) {
    let mut local = in_local;
    if let LayerNodeKind::Group { group_kind, .. } = &node.kind {
        if matches!(
            group_kind,
            GroupKind::TextBox | GroupKind::Header | GroupKind::Footer | GroupKind::FootnoteArea | GroupKind::MasterPage
        ) {
            if !in_local {
                // boundary: can the context ROOT be resolved to its owning control?
                g.roots += 1;
                let resolved = node
                    .source_node_id
                    .and_then(|id| render.get(&id))
                    .and_then(|rn| find_provenance(rn))
                    .map(|(s, p, _)| doc.sections.get(s).map(|sec| p < sec.paragraphs.len()).unwrap_or(false))
                    .unwrap_or(false);
                if resolved {
                    g.roots_resolved += 1;
                } else {
                    g.root_kinds_unresolved.push(format!("{group_kind:?}").chars().take(20).collect());
                }
            }
            local = true;
        }
    }
    if local {
        g.local_nodes += 1;
        if let Some(id) = node.source_node_id {
            g.with_source_id += 1;
            if let Some(rn) = render.get(&id) {
                g.id_in_render += 1;
                if let Some((s, p, _c)) = find_provenance(rn) {
                    g.provenance_found += 1;
                    if doc.sections.get(s).map(|sec| p < sec.paragraphs.len()).unwrap_or(false) {
                        g.provenance_valid += 1;
                    }
                }
            }
        }
    }
    match &node.kind {
        LayerNodeKind::Group { children, .. } => {
            for ch in children {
                gap1_walk(ch, doc, render, local, g);
            }
        }
        LayerNodeKind::ClipRect { child, .. } => gap1_walk(child, doc, render, local, g),
        LayerNodeKind::Leaf { .. } => {}
    }
}

fn gap1(doc: &Document, core: &DocumentCore) {
    let mut g = Gap1::default();
    for page in 0..core.page_count() {
        let render_tree = core.build_page_render_tree(page).expect("render tree");
        let layer_tree = core.build_page_layer_tree(page).expect("layer tree");
        let mut map = HashMap::new();
        index_render(&render_tree.root, &mut map);
        gap1_walk(&layer_tree.root, doc, &map, false, &mut g);
    }
    println!("gap1: local-context layer nodes (incl. descendants): {}", g.local_nodes);
    println!("  with source_node_id:            {}", g.with_source_id);
    println!("  id found in render tree:        {}", g.id_in_render);
    println!("  provenance found (node/desc):   {}", g.provenance_found);
    println!("  provenance valid in model:      {}", g.provenance_valid);
    println!("  local ROOT groups: {} | resolved to owning control: {}", g.roots, g.roots_resolved);
    if !g.root_kinds_unresolved.is_empty() {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for k in &g.root_kinds_unresolved {
            *counts.entry(k.clone()).or_default() += 1;
        }
        println!("  unresolved root kinds: {counts:?}");
    }
}

// ---------- gap2 ----------

fn gap2(doc: &Document, core: &DocumentCore) {
    let body_len: usize = doc.sections[0].paragraphs.len();
    let mut note_paras: Vec<String> = Vec::new(); // appended order: footnotes? endnotes per doc order
    for (pi, para) in doc.sections[0].paragraphs.iter().enumerate() {
        for (ci, ctrl) in para.controls.iter().enumerate() {
            match ctrl {
                Control::Endnote(e) => {
                    for (k, np) in e.paragraphs.iter().enumerate() {
                        note_paras.push(format!(
                            "endnote@para{pi}/ctrl{ci} note_para{k}: {:?}",
                            np.text.chars().take(28).collect::<String>()
                        ));
                    }
                }
                Control::Footnote(f) => {
                    for (k, np) in f.paragraphs.iter().enumerate() {
                        note_paras.push(format!(
                            "footnote@para{pi}/ctrl{ci} note_para{k}: {:?}",
                            np.text.chars().take(28).collect::<String>()
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    println!("gap2: section0 body paragraphs: {body_len}");
    println!("note paragraphs (doc order, {} total):", note_paras.len());
    for s in &note_paras {
        println!("  {s}");
    }
    // collect out-of-range body TextLines from layer trees, with their source render nodes
    for page in 0..core.page_count() {
        let layer = core.build_page_layer_tree(page).expect("layer");
        let render = core.build_page_render_tree(page).expect("render");
        let mut map = HashMap::new();
        index_render(&render.root, &mut map);
        collect_oor(&layer.root, body_len, page, &map);
    }
}

fn collect_oor(node: &LayerNode, body_len: usize, page: u32, render: &HashMap<NodeId, &RenderNode>) {
    if let LayerNodeKind::Group { group_kind, children, .. } = &node.kind {
        if let GroupKind::TextLine(t) = group_kind {
            if let (Some(s), Some(p)) = (t.section_index, t.para_index) {
                if p != usize::MAX && p >= body_len {
                    let src = node
                        .source_node_id
                        .and_then(|id| render.get(&id))
                        .map(|rn| format!("{:?}", std::mem::discriminant(&rn.node_type)));
                    println!(
                        "  OOR line page={page} sec={s} para={p} (= body_len + {}) src_node={:?} bounds_y={:.0}",
                        p - body_len, src, node.bounds.y
                    );
                }
            }
        }
        for ch in children {
            collect_oor(ch, body_len, page, render);
        }
    } else if let LayerNodeKind::ClipRect { child, .. } = &node.kind {
        collect_oor(child, body_len, page, render);
    }
}

// ---------- gap3 ----------

fn gap3(doc: &Document, core: &DocumentCore) {
    let para = &doc.sections[0].paragraphs[15];
    println!("gap3: model sections[0].paragraphs[15] — text={:?}, controls={}", para.text.chars().take(30).collect::<String>(), para.controls.len());
    for (ci, ctrl) in para.controls.iter().enumerate() {
        match ctrl {
            Control::Table(t) => {
                println!("  ctrl[{ci}] = Table {}x{} cells={}", t.row_count, t.col_count, t.cells.len());
                for (idx, cell) in t.cells.iter().enumerate() {
                    for (cpi, cp) in cell.paragraphs.iter().enumerate() {
                        for (cci, cc) in cp.controls.iter().enumerate() {
                            if let Control::Table(nt) = cc {
                                println!(
                                    "    cells[{idx}].paragraphs[{cpi}].controls[{cci}] = nested Table {}x{}",
                                    nt.row_count, nt.col_count
                                );
                            }
                        }
                    }
                }
            }
            other => println!("  ctrl[{ci}] = {:?}", std::mem::discriminant(other)),
        }
    }
    // layer side: all Table nodes referencing (0,15,*)
    for page in 0..core.page_count() {
        let layer = core.build_page_layer_tree(page).expect("layer");
        gap3_walk(&layer.root, page, &mut Vec::new());
    }
}

fn gap3_walk(node: &LayerNode, page: u32, path: &mut Vec<String>) {
    if let LayerNodeKind::Group { group_kind, children, .. } = &node.kind {
        let label = match group_kind {
            GroupKind::Table(t) => {
                if t.section_index == Some(0) && t.para_index == Some(15) {
                    println!(
                        "  layer Table page={page} ref=(0,15,{:?}) dims={}x{} bounds=({:.0},{:.0} {:.0}x{:.0}) path={}",
                        t.control_index, t.row_count, t.col_count,
                        node.bounds.x, node.bounds.y, node.bounds.width, node.bounds.height,
                        path.join(">")
                    );
                }
                format!("Table({:?},{:?},{:?})", t.section_index, t.para_index, t.control_index)
            }
            GroupKind::TableCell(c) => format!("Cell(idx={:?})", c.model_cell_index),
            GroupKind::TextBox => "TextBox".into(),
            GroupKind::Header => "Header".into(),
            GroupKind::Footer => "Footer".into(),
            GroupKind::MasterPage => "MasterPage".into(),
            GroupKind::Column(n) => format!("Col{n}"),
            GroupKind::Body => "Body".into(),
            _ => String::new(),
        };
        let pushed = !label.is_empty();
        if pushed {
            path.push(label);
        }
        for ch in children {
            gap3_walk(ch, page, path);
        }
        if pushed {
            path.pop();
        }
    } else if let LayerNodeKind::ClipRect { child, .. } = &node.kind {
        gap3_walk(child, page, path);
    }
}

fn main() {
    let mode = std::env::args().nth(1).expect("usage: gap_probe <gap1|gap2|gap3> <input.hwp>");
    let path = std::env::args().nth(2).expect("usage: gap_probe <gap1|gap2|gap3> <input.hwp>");
    let data = std::fs::read(&path).expect("read input");
    let doc = rhwp::parse_document(&data).expect("parse_document");
    let core = DocumentCore::from_bytes(&data).expect("from_bytes");

    match mode.as_str() {
        "gap1" => gap1(&doc, &core),
        "gap2" => gap2(&doc, &core),
        "gap3" => gap3(&doc, &core),
        m => panic!("unknown mode {m}"),
    }
}
