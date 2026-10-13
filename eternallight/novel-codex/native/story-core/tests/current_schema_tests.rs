use novel_codex_story_core::SqliteStoryRepository;
use rusqlite::Connection;
#[test]
fn current_schema_initializes_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.sqlite3");
    drop(SqliteStoryRepository::open(&path).unwrap());
    drop(SqliteStoryRepository::open(&path).unwrap());
}
#[test]
fn malformed_current_table_is_rejected_before_initialization_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("malformed.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE story_work_tasks_v1(id TEXT PRIMARY KEY); INSERT INTO story_work_tasks_v1 VALUES('retained');").unwrap();
    let before: i64 = db
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap();
    let error = match SqliteStoryRepository::open(&path) {
        Ok(_) => panic!("invalid schema accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("CURRENT_SCHEMA_MISMATCH"));
    assert_eq!(
        db.query_row::<i64, _, _>("PRAGMA schema_version", [], |r| r.get(0))
            .unwrap(),
        before
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT id FROM story_work_tasks_v1", [], |r| r.get(0))
            .unwrap(),
        "retained"
    );
}

fn rejects_change_without_writes(change: impl FnOnce(&Connection)) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("current.sqlite3");
    drop(SqliteStoryRepository::open(&path).unwrap());
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE retained_author_data(value TEXT); INSERT INTO retained_author_data VALUES ('retained');").unwrap();
    change(&db);
    let before: i64 = db
        .query_row("PRAGMA schema_version", [], |r| r.get(0))
        .unwrap();
    let error = match SqliteStoryRepository::open(&path) {
        Ok(_) => panic!("invalid schema accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("CURRENT_SCHEMA_MISMATCH"));
    assert_eq!(
        db.query_row::<i64, _, _>("PRAGMA schema_version", [], |r| r.get(0))
            .unwrap(),
        before
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT value FROM retained_author_data", [], |r| r.get(0))
            .unwrap(),
        "retained"
    );
}

#[test]
fn module_table_column_mismatch_is_rejected() {
    rejects_change_without_writes(|db| {
        db.execute_batch("ALTER TABLE story_model_dependencies_v1 ADD COLUMN obsolete TEXT")
            .unwrap()
    });
}

#[test]
fn missing_module_table_is_not_silently_recreated() {
    rejects_change_without_writes(|db| {
        db.execute_batch("DROP TABLE story_model_dependencies_v1")
            .unwrap()
    });
}

#[test]
fn index_definition_mismatch_is_rejected() {
    rejects_change_without_writes(|db| {
        db.execute_batch("DROP INDEX story_model_dependencies_source_v1; CREATE INDEX story_model_dependencies_source_v1 ON story_model_dependencies_v1(batch_id)").unwrap()
    });
}

#[test]
fn trigger_definition_mismatch_is_rejected() {
    rejects_change_without_writes(|db| {
        db.execute_batch("DROP TRIGGER story_model_dependencies_insert_v1; CREATE TRIGGER story_model_dependencies_insert_v1 AFTER INSERT ON story_modeling_batches_v2 BEGIN SELECT 1; END").unwrap()
    });
}

#[test]
fn same_columns_with_different_constraint_are_rejected() {
    rejects_change_without_writes(|db| {
        // Edit the schema catalog only in this isolated corruption fixture.
        db.execute_batch("PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'PRIMARY KEY(project_id,batch_id,source_batch_id)','UNIQUE(project_id,batch_id,source_batch_id)') WHERE name='story_model_dependencies_v1'; PRAGMA writable_schema=OFF;").unwrap();
    });
}

#[test]
fn unexpected_trigger_on_owned_table_is_rejected_even_without_prefix() {
    rejects_change_without_writes(|db| {
        db.execute_batch("CREATE TRIGGER obsolete_behavior AFTER INSERT ON story_projects_v2 BEGIN SELECT RAISE(ABORT, 'obsolete behavior'); END").unwrap()
    });
}

#[test]
fn sqlite_case_insensitive_names_cannot_bypass_validation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("uppercase.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE STORY_PROJECTS_V2(obsolete TEXT); INSERT INTO STORY_PROJECTS_V2 VALUES('retained')").unwrap();
    let before: i64 = db
        .query_row("PRAGMA schema_version", [], |row| row.get(0))
        .unwrap();
    let error = match SqliteStoryRepository::open(&path) {
        Ok(_) => panic!("invalid uppercase schema accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("CURRENT_SCHEMA_MISMATCH"));
    assert_eq!(
        db.query_row::<i64, _, _>("PRAGMA schema_version", [], |row| row.get(0))
            .unwrap(),
        before
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT obsolete FROM STORY_PROJECTS_V2", [], |row| row
            .get(0))
            .unwrap(),
        "retained"
    );
}
