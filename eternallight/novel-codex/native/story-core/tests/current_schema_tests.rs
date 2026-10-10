use novel_codex_story_core::SqliteStoryRepository;
use rusqlite::Connection;
#[test]
fn current_schema_initializes_and_reopens() {
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("current.sqlite3");
    drop(SqliteStoryRepository::open(&path).unwrap());
    drop(SqliteStoryRepository::open(&path).unwrap());
}
#[test]
fn malformed_current_table_is_rejected_before_initialization_writes() {
    let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("malformed.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY); INSERT INTO story_work_tasks_v1 VALUES('retained');").unwrap();
    let before: i64 = db.query_row("PRAGMA schema_version", [], |r| r.get(0)).unwrap();
    let error = match SqliteStoryRepository::open(&path) { Ok(_) => panic!("invalid schema accepted"), Err(error) => error };
    assert!(format!("{error:?}").contains("CURRENT_SCHEMA_MISMATCH"));
    assert_eq!(db.query_row::<i64,_,_>("PRAGMA schema_version", [], |r|r.get(0)).unwrap(), before);
    assert_eq!(db.query_row::<String,_,_>("SELECT id FROM story_work_tasks_v1", [], |r|r.get(0)).unwrap(), "retained");
}
