//! Validate existing base tables against the current DDL before any initialization writes.
//! This deliberately does not repair, migrate or infer defaults for existing data.
use rusqlite::{Connection, OptionalExtension};
use std::sync::OnceLock;
use crate::RepositoryError;
use crate::authoring_runs::storage_error;

type Column = (String, String, bool, i64);
fn columns(db: &Connection, table: &str) -> Result<Vec<Column>, RepositoryError> {
    let mut query = db.prepare("SELECT name,type,\"notnull\",pk FROM pragma_table_info(?1) ORDER BY cid").map_err(storage_error)?;
    query.query_map([table], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(storage_error)?.collect::<Result<Vec<_>, _>>().map_err(storage_error)
}
pub(crate) fn validate(db: &Connection, ddl: &'static str) -> Result<(), RepositoryError> {
    static EXPECTED: OnceLock<Result<Vec<(String, Vec<Column>)>, String>> = OnceLock::new();
    let expected = EXPECTED.get_or_init(|| {
        let build = || -> Result<_, RepositoryError> {
            let reference = Connection::open_in_memory().map_err(storage_error)?;
            reference.execute_batch(ddl).map_err(storage_error)?;
            let names = reference.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
                .map_err(storage_error)?.query_map([], |r| r.get::<_, String>(0)).map_err(storage_error)?
                .collect::<Result<Vec<_>, _>>().map_err(storage_error)?;
            names.into_iter().map(|name| Ok((name.clone(), columns(&reference, &name)?))).collect()
        };
        build().map_err(|e| format!("{e:?}"))
    }).as_ref().map_err(|e| RepositoryError::Storage(e.clone()))?;
    for (name, wanted) in expected {
        let exists: Option<i64> = db.query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1", [name], |r| r.get(0))
            .optional().map_err(storage_error)?;
        if exists.is_some() && columns(db, name)? != *wanted {
            return Err(RepositoryError::Storage(format!(
                "CURRENT_SCHEMA_MISMATCH: 数据库表 {name} 不符合当前版本结构；未修改原数据。请使用当前版本初始化的独立工作区。")));
        }
    }
    Ok(())
}
