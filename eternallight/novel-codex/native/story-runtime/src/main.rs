use std::env;
use std::io;
use std::io::BufRead;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use novel_codex_story_runtime::RuntimeState;
use novel_codex_story_runtime::router;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Credentials {
    candidate_token: String,
    author_token: String,
}

struct Arguments {
    host: Ipv4Addr,
    port: u16,
    db: PathBuf,
    engine_budget_db: PathBuf,
    token_stdin: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = arguments()?;
    if arguments.host != Ipv4Addr::LOCALHOST {
        return Err("Story Runtime only supports loopback host 127.0.0.1".into());
    }
    if !arguments.token_stdin {
        return Err("--token-stdin is required".into());
    }
    let mut credentials_line = String::new();
    io::stdin().lock().read_line(&mut credentials_line)?;
    if credentials_line.len() > 4096 {
        return Err("Story Runtime credentials exceed 4096 bytes".into());
    }
    let credentials: Credentials = serde_json::from_str(&credentials_line)?;
    if credentials.author_token.len() < 24
        || credentials.candidate_token.len() < 24
        || credentials.author_token == credentials.candidate_token
    {
        return Err("Story Runtime tokens must be distinct and at least 24 characters".into());
    }
    if let Some(parent) = arguments.db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    novel_codex_story_core::SqliteStoryRepository::open(&arguments.db)?;
    let audit_database = arguments.db.clone();
    let entity_audit_database = arguments.db.clone();
    let relation_audit_database = arguments.db.clone();
    let knowledge_audit_database = arguments.db.clone();
    let snapshot_knowledge_audit_database = arguments.db.clone();
    let fact_audit_database = arguments.db.clone();
    let condition_database = arguments.db.clone();
    let state = RuntimeState::new(
        arguments.db,
        credentials.author_token,
        credentials.candidate_token,
    )
    .with_engine_budget_database(arguments.engine_budget_db);
    let listener = tokio::net::TcpListener::bind((arguments.host, arguments.port)).await?;
    std::thread::Builder::new()
        .name("story-search-audit".into())
        .spawn(move || {
            loop {
                let delay = match novel_codex_story_runtime::search_audit_tick(&audit_database) {
                    Ok(Some(_)) => Duration::from_millis(25),
                    Ok(None) => Duration::from_millis(250),
                    Err(error) => {
                        eprintln!("Story search audit worker: {error}");
                        Duration::from_secs(1)
                    }
                };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-entity-audit".into())
        .spawn(move || {
            loop {
                let delay =
                    match novel_codex_story_runtime::entity_audit_tick(&entity_audit_database) {
                        Ok(Some(_)) => Duration::from_millis(25),
                        Ok(None) => Duration::from_millis(250),
                        Err(error) => {
                            eprintln!("Story entity audit worker: {error}");
                            Duration::from_secs(1)
                        }
                    };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-relation-audit".into())
        .spawn(move || {
            loop {
                let delay = match novel_codex_story_runtime::relation_audit_tick(
                    &relation_audit_database,
                ) {
                    Ok(Some(_)) => Duration::from_millis(25),
                    Ok(None) => Duration::from_millis(250),
                    Err(error) => {
                        eprintln!("Story relation audit worker: {error}");
                        Duration::from_secs(1)
                    }
                };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-knowledge-audit".into())
        .spawn(move || {
            loop {
                let delay = match novel_codex_story_runtime::knowledge_audit_tick(
                    &knowledge_audit_database,
                ) {
                    Ok(Some(_)) => Duration::from_millis(25),
                    Ok(None) => Duration::from_millis(250),
                    Err(error) => {
                        eprintln!("Story knowledge audit worker: {error}");
                        Duration::from_secs(1)
                    }
                };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-snapshot-knowledge-audit".into())
        .spawn(move || {
            loop {
                let delay = match novel_codex_story_runtime::snapshot_knowledge_audit_tick(
                    &snapshot_knowledge_audit_database,
                ) {
                    Ok(Some(_)) => Duration::from_millis(25),
                    Ok(None) => Duration::from_millis(250),
                    Err(error) => {
                        eprintln!("Story snapshot knowledge audit worker: {error}");
                        Duration::from_secs(1)
                    }
                };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-fact-audit".into())
        .spawn(move || {
            loop {
                let delay = match novel_codex_story_runtime::fact_audit_tick(&fact_audit_database) {
                    Ok(Some(_)) => Duration::from_millis(25),
                    Ok(None) => Duration::from_millis(250),
                    Err(error) => {
                        eprintln!("Story fact audit worker: {error}");
                        Duration::from_secs(1)
                    }
                };
                std::thread::sleep(delay);
            }
        })?;
    std::thread::Builder::new()
        .name("story-condition-evaluation".into())
        .spawn(move || {
            loop {
                let delay =
                    match novel_codex_story_runtime::condition_evaluation_tick(&condition_database)
                    {
                        Ok(true) => Duration::from_millis(25),
                        Ok(false) => Duration::from_millis(250),
                        Err(error) => {
                            eprintln!("Story condition evaluation worker: {error}");
                            Duration::from_secs(1)
                        }
                    };
                std::thread::sleep(delay);
            }
        })?;
    println!(
        "Story Runtime listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn arguments() -> Result<Arguments, Box<dyn std::error::Error>> {
    let mut host = Ipv4Addr::LOCALHOST;
    let mut port = 8765;
    let mut db = None;
    let mut token_stdin = false;
    let mut engine_budget_db = None;
    let mut values = env::args().skip(1);
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--host" => host = values.next().ok_or("--host requires a value")?.parse()?,
            "--port" => port = values.next().ok_or("--port requires a value")?.parse()?,
            "--db" => db = Some(PathBuf::from(values.next().ok_or("--db requires a value")?)),
            "--engine-budget-db" => {
                engine_budget_db = Some(PathBuf::from(
                    values.next().ok_or("--engine-budget-db requires a value")?,
                ))
            }
            "--token-stdin" => token_stdin = true,
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    Ok(Arguments {
        host,
        port,
        db: db.ok_or("--db is required")?,
        engine_budget_db: engine_budget_db.ok_or("--engine-budget-db is required")?,
        token_stdin,
    })
}
