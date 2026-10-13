//! Compare every owned schema object before any initialization writes.
use crate::{RepositoryError, authoring_runs::storage_error};
use rusqlite::Connection;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(PartialEq, Eq)]
struct Definition {
    owner: String,
    tokens: Vec<String>,
}
type Schema = BTreeMap<(String, String), Definition>;

// SQL formatting is irrelevant; quoted literals retain their exact value.
fn tokens(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        let start = i;
        i += 1;
        if matches!(c, '\'' | '"' | '`' | '[') {
            let end = if c == '[' { ']' } else { c };
            while i < chars.len() {
                if chars[i] == end {
                    i += 1;
                    if end != ']' && chars.get(i) == Some(&end) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            result.push(chars[start..i].iter().collect());
        } else if c.is_alphanumeric() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            result.push(
                chars[start..i]
                    .iter()
                    .collect::<String>()
                    .to_ascii_uppercase(),
            );
        } else {
            result.push(c.to_string());
        }
    }
    while result.last().is_some_and(|s| s == ";") {
        result.pop();
    }
    result
}

fn objects(db: &Connection) -> Result<Schema, RepositoryError> {
    let mut query = db.prepare("SELECT type,name,sql,tbl_name FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name").map_err(storage_error)?;
    query
        .query_map([], |r| {
            Ok((
                (
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?.to_ascii_lowercase(),
                ),
                Definition {
                    owner: r.get::<_, String>(3)?.to_ascii_lowercase(),
                    tokens: tokens(&r.get::<_, String>(2)?),
                },
            ))
        })
        .map_err(storage_error)?
        .collect::<Result<_, _>>()
        .map_err(storage_error)
}

pub(crate) fn validate(db: &Connection) -> Result<(), RepositoryError> {
    static EXPECTED: OnceLock<Result<Schema, String>> = OnceLock::new();
    let expected = EXPECTED
        .get_or_init(|| {
            let build = || -> Result<_, RepositoryError> {
                let reference = Connection::open_in_memory().map_err(storage_error)?;
                reference
                    .execute_batch(crate::schema::ddl())
                    .map_err(storage_error)?;
                objects(&reference)
            };
            build().map_err(|e| format!("{e:?}"))
        })
        .as_ref()
        .map_err(|e| RepositoryError::Storage(e.clone()))?;
    let actual = objects(db)?;
    for (key, definition) in &actual {
        if let Some(wanted) = expected.get(key) {
            if definition != wanted {
                return Err(mismatch(&key.0, &key.1));
            }
        } else if key.1.starts_with("story_") || definition.owner.starts_with("story_") {
            return Err(mismatch(&key.0, &key.1));
        }
    }
    // Missing objects in an existing store are rejected, not silently repaired.
    if actual.keys().any(|(_, name)| name.starts_with("story_")) {
        for key in expected.keys() {
            if !actual.contains_key(key) {
                return Err(mismatch(&key.0, &key.1));
            }
        }
    }
    Ok(())
}

fn mismatch(kind: &str, name: &str) -> RepositoryError {
    RepositoryError::Storage(format!(
        "CURRENT_SCHEMA_MISMATCH: 数据库{kind} {name} 不符合当前版本结构；未修改原数据。请使用当前版本初始化的独立工作区。"
    ))
}
