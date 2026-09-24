use std::env;
use std::io;
use std::io::BufRead;
use std::net::Ipv4Addr;
use std::path::PathBuf;

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
    let state = RuntimeState::new(
        arguments.db,
        credentials.author_token,
        credentials.candidate_token,
    );
    let listener = tokio::net::TcpListener::bind((arguments.host, arguments.port)).await?;
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
    let mut values = env::args().skip(1);
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--host" => host = values.next().ok_or("--host requires a value")?.parse()?,
            "--port" => port = values.next().ok_or("--port requires a value")?.parse()?,
            "--db" => db = Some(PathBuf::from(values.next().ok_or("--db requires a value")?)),
            "--token-stdin" => token_stdin = true,
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    Ok(Arguments {
        host,
        port,
        db: db.ok_or("--db is required")?,
        token_stdin,
    })
}
