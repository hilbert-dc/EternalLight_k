//! Measures production retrieval against external annotations without modifying the source corpus.
use std::io::Read;
use std::path::{Path, PathBuf};

use novel_codex_story_core::SqliteStoryRepository;
use rusqlite::{Connection, OpenFlags};
use sha2::{Digest, Sha256};

#[path = "support/story_evaluation.rs"]
mod evaluation;

fn evaluate(database: &Path, annotations: &Path) -> evaluation::Result<serde_json::Value> {
    let mut bytes = Vec::new();
    std::fs::File::open(annotations)?
        .take(1_048_577)
        .read_to_end(&mut bytes)?;
    let dataset = evaluation::parse_dataset(&bytes)?;
    let directory = tempfile::tempdir()?;
    let copy_path = directory.path().join("故事评估.sqlite3");
    // Read-only VACUUM INTO captures committed WAL data and never initializes the author's schema.
    let source = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.busy_timeout(std::time::Duration::from_secs(5))?;
    source.execute(
        "VACUUM INTO ?1",
        [copy_path.to_str().ok_or("invalid local path")?],
    )?;
    drop(source);
    let mut hash = Sha256::new();
    let mut file = std::fs::File::open(&copy_path)?;
    let mut buffer = [0; 16_384];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    let corpus_digest = format!("{:x}", hash.finalize());
    let repository = SqliteStoryRepository::open(&copy_path)?;
    let captured = Connection::open_with_flags(&copy_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    evaluation::measure(&repository, &captured, &dataset, &bytes, &corpus_digest)
}

fn main() {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 2 {
        eprintln!("usage: story_retrieval_evaluation <database> <annotations.json>");
        std::process::exit(1);
    }
    match evaluate(&args[0], &args[1]) {
        Ok(report) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("report is JSON")
            );
            if report["measuredThresholdsMet"] != true {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("story evaluation rejected: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
#[path = "support/story_evaluation_tests.rs"]
mod tests;
