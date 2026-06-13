//! Throwaway probe: report column-break markers per fixture.
use rhwp::model::paragraph::ColumnBreakType;
use rhwp::parser::parse_document;

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).expect("read");
        match parse_document(&data) {
            Ok(doc) => {
                let mut counts = std::collections::HashMap::new();
                let mut col_def_counts = 0usize;
                for (si, sec) in doc.sections.iter().enumerate() {
                    for (pi, para) in sec.paragraphs.iter().enumerate() {
                        if para.column_type != ColumnBreakType::None {
                            *counts.entry(format!("{:?}", para.column_type)).or_insert(0) += 1;
                            eprintln!("  s{}/p{}: column_type={:?}", si, pi, para.column_type);
                        }
                        for ctrl in &para.controls {
                            if let rhwp::model::control::Control::ColumnDef(c) = ctrl {
                                col_def_counts += 1;
                                eprintln!("  s{}/p{}: ColumnDef count={}", si, pi, c.column_count);
                            }
                        }
                    }
                }
                println!("{path}: sections={} breaks={:?} column_defs={}", doc.sections.len(), counts, col_def_counts);
            }
            Err(e) => println!("{path}: PARSE ERROR {e}"),
        }
    }
}
