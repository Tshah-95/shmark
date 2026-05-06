//! Quick walker debug — runs the live search against real ~/Github and dumps
//! everything it sees. Use to understand why a query returns fewer hits than
//! expected.

use shmark_core::resolve::{default_roots, resolve, Resolution};

fn main() {
    let roots = default_roots();
    println!("roots: {roots:?}");

    let query = std::env::args().nth(1).expect("usage: debug_search <query>");
    println!("query: {query:?}");

    let res = resolve(&query, &roots);
    match &res {
        Resolution::Path { candidate } => {
            println!("→ Path: {}", candidate.path);
        }
        Resolution::Candidates { candidates } => {
            println!("→ {} candidates:", candidates.len());
            for c in candidates {
                println!("   {}", c.path);
            }
        }
        other => println!("→ {other:?}"),
    }

    // Direct walker dump for the first root, no query filter.
    println!("\n--- direct walker dump (first root, top 30 file hits) ---");
    if let Some(first) = roots.first() {
        let mut wb = ignore::WalkBuilder::new(first);
        wb.max_depth(Some(10))
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .add_custom_ignore_filename(".gitignore");
        let mut count = 0;
        for entry in wb.build() {
            let Ok(entry) = entry else {
                continue;
            };
            let Some(ft) = entry.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let p = entry.path();
            if p.file_name().map(|n| n == query.as_str()).unwrap_or(false) {
                println!("  match: {}", p.display());
            }
            count += 1;
            if count >= 30000 {
                println!("  (stopping walker dump at 30000 files)");
                break;
            }
        }
        println!("walked {count} files in {}", first.display());
    }
}
