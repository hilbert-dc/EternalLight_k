//! Agent self-assessment bound to persisted work; never a literary certificate.
use crate::authoring_runs::storage_error;
use crate::{RepositoryError, TaskRecord};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Review {
    expected_work_version: i64,
    expected_source_digest: String,
    goal_status: String,
    goal_assessment: String,
    checked_scope: String,
    unresolved: Vec<String>,
    author_decisions: Vec<String>,
    evidence: Vec<Evidence>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    task_id: String,
    revision_id: String,
}
fn invalid(detail: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("deliveryReview: {detail}"))
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max
}
pub(crate) fn save(
    db: &Connection,
    task: &TaskRecord,
    work: &str,
    status: &Value,
    value: Option<&Value>,
) -> Result<(), RepositoryError> {
    let value = value.ok_or_else(|| invalid("final decision requires a delivery review"))?;
    let review: Review =
        serde_json::from_value(value.clone()).map_err(|_| invalid("invalid review fields"))?;
    if status["version"] != review.expected_work_version {
        return Err(RepositoryError::WorkTaskStale);
    }
    if status["sourceDigest"] != review.expected_source_digest {
        return Err(RepositoryError::TaskInputStale);
    }
    if !matches!(
        review.goal_status.as_str(),
        "fulfilled" | "partial" | "needsAuthorDecision"
    ) || !text(&review.goal_assessment, 1600)
        || !text(&review.checked_scope, 1600)
        || review.unresolved.len() > 16
        || review.author_decisions.len() > 16
        || review.evidence.len() > 1024
        || review
            .unresolved
            .iter()
            .chain(&review.author_decisions)
            .any(|s| !text(s, 600))
    {
        return Err(invalid(
            "review exceeds bounds or has empty required content",
        ));
    }
    if (review.goal_status == "fulfilled"
        && (!review.unresolved.is_empty() || !review.author_decisions.is_empty()))
        || (review.goal_status == "partial" && review.unresolved.is_empty())
        || (review.goal_status == "needsAuthorDecision" && review.author_decisions.is_empty())
    {
        return Err(invalid(
            "goal status must match unresolved work and author decisions",
        ));
    }
    let mut refs = BTreeSet::new();
    for reference in &review.evidence {
        if !text(&reference.task_id, 160)
            || !text(&reference.revision_id, 160)
            || !refs.insert((&reference.task_id, &reference.revision_id))
        {
            return Err(invalid("invalid or duplicate evidence"));
        }
        let valid: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.task_id=?3 AND t.status='completed' AND o.revision_id=?4)",params![work,task.project_id.as_str(),reference.task_id,reference.revision_id],|r|r.get(0)).map_err(storage_error)?;
        if !valid {
            return Err(invalid("evidence is not a completed output of this Work"));
        }
    }
    // Cover every currently selected revision saved by this Work, not only
    // the last page of receipts; obsolete drafts must not stand in for current text.
    let mut query=db.prepare("SELECT DISTINCT t.task_id,o.revision_id FROM story_work_steps_v1 s JOIN story_tasks_v2 t ON t.task_id=s.current_attempt_id JOIN story_task_outputs_v2 o ON o.task_id=t.task_id JOIN story_object_heads_v2 h ON h.project_id=t.project_id AND COALESCE(h.candidate_revision_id,h.accepted_revision_id)=o.revision_id WHERE s.work_task_id=?1 AND t.project_id=?2 AND t.status='completed' AND (o.output_kind!='draft' OR o.completeness='chapterComplete')").map_err(storage_error)?;
    let required = query
        .query_map(params![work, task.project_id.as_str()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage_error)?;
    for item in required {
        let (task_id, revision) = item.map_err(storage_error)?;
        if !refs.contains(&(&task_id, &revision)) {
            return Err(invalid(
                "review must cover all current saved revision outputs",
            ));
        }
    }
    db.execute("INSERT INTO story_work_delivery_reviews_v1(decision_task_id,work_task_id,content_json) VALUES(?1,?2,?3)",params![task.task_id,work,value.to_string()]).map_err(storage_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn setup() -> (Connection, TaskRecord, Value) {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_work_steps_v1(work_task_id TEXT,current_attempt_id TEXT); CREATE TABLE story_tasks_v2(task_id TEXT,project_id TEXT,status TEXT); CREATE TABLE story_task_outputs_v2(task_id TEXT,revision_id TEXT,output_kind TEXT,completeness TEXT); CREATE TABLE story_object_heads_v2(project_id TEXT,candidate_revision_id TEXT,accepted_revision_id TEXT); CREATE TABLE story_work_delivery_reviews_v1(decision_task_id TEXT PRIMARY KEY,work_task_id TEXT,content_json TEXT); INSERT INTO story_work_steps_v1 VALUES('work','writer'); INSERT INTO story_tasks_v2 VALUES('writer','project','completed'); INSERT INTO story_task_outputs_v2 VALUES('writer','revision','draft','chapterComplete'); INSERT INTO story_object_heads_v2 VALUES('project','revision',NULL);").unwrap();
        let task = TaskRecord {
            task_id: "decision".into(),
            project_id: crate::ProjectId::parse("project").unwrap(),
            thread_id: "thread".into(),
            operation: "advisory".into(),
            task_kind: "advisory".into(),
            purpose: "advisory".into(),
            required_output_kind: None,
            target_json: "{}".into(),
            input_manifest_json: "{}".into(),
            input_revision: 0,
            model: None,
            turn_id: None,
            status: "running".into(),
            stage: "running".into(),
            delivery_status: "pending".into(),
            output_refs_json: "[]".into(),
            result_summary: "".into(),
            error: None,
            capability_hash: "".into(),
            capability_expires_at: 0,
            capability_revoked_at: None,
        };
        let review = json!({"expectedWorkVersion":2,"expectedSourceDigest":"digest","goalStatus":"fulfilled","goalAssessment":"已按作者要求修正这一句","checkedScope":"仅核对本次句子与上下文，未检查全书","unresolved":[],"authorDecisions":[],"evidence":[{"taskId":"writer","revisionId":"revision"}]});
        (db, task, review)
    }
    fn status() -> Value {
        json!({"version":2,"sourceDigest":"digest"})
    }
    #[test]
    fn rejects_missing_stale_forged_and_incomplete_reviews_without_writes() {
        let (db, task, review) = setup();
        assert!(save(&db, &task, "work", &status(), None).is_err());
        for (field, value) in [
            ("expectedWorkVersion", json!(1)),
            ("expectedSourceDigest", json!("old")),
            ("evidence", json!([])),
            (
                "evidence",
                json!([{"taskId":"foreign","revisionId":"revision"}]),
            ),
            ("unresolved", json!(["尚未改完"])),
        ] {
            let mut invalid = review.clone();
            invalid[field] = value;
            assert!(
                save(&db, &task, "work", &status(), Some(&invalid)).is_err(),
                "{field}"
            );
        }
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        save(&db, &task, "work", &status(), Some(&review)).unwrap();
        let stored: String = db
            .query_row(
                "SELECT content_json FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), review);
    }
    #[test]
    fn accepts_explicit_partial_outcome_and_read_only_work_without_fabricated_evidence() {
        let (db, task, mut review) = setup();
        review["goalStatus"] = json!("partial");
        review["unresolved"] = json!(["后续人物冲突仍需另行处理"]);
        save(&db, &task, "work", &status(), Some(&review)).unwrap();
        db.execute("DELETE FROM story_work_delivery_reviews_v1", [])
            .unwrap();
        db.execute("DELETE FROM story_work_steps_v1", []).unwrap();
        review["evidence"] = json!([]);
        review["goalStatus"] = json!("needsAuthorDecision");
        review["authorDecisions"] = json!(["作者选择保留哪个结局"]);
        save(&db, &task, "work", &status(), Some(&review)).unwrap();
    }
    #[test]
    fn current_planning_output_requires_evidence_even_when_prose_is_covered() {
        let (db, task, mut review) = setup();
        db.execute_batch("INSERT INTO story_work_steps_v1 VALUES('work','planner'); INSERT INTO story_tasks_v2 VALUES('planner','project','completed'); INSERT INTO story_task_outputs_v2 VALUES('planner','brief','brief',NULL); INSERT INTO story_object_heads_v2 VALUES('project','brief',NULL);").unwrap();
        assert!(save(&db, &task, "work", &status(), Some(&review)).is_err());
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        review["evidence"]
            .as_array_mut()
            .unwrap()
            .push(json!({"taskId":"planner","revisionId":"brief"}));
        save(&db, &task, "work", &status(), Some(&review)).unwrap();
    }
    #[test]
    fn hundred_chapter_review_covers_all_current_prose_and_optional_reports() {
        let (db, task, mut review) = setup();
        db.execute_batch("DELETE FROM story_work_steps_v1; DELETE FROM story_tasks_v2; DELETE FROM story_task_outputs_v2; DELETE FROM story_object_heads_v2;").unwrap();
        let mut evidence = Vec::new();
        for chapter in 1..=100 {
            for kind in ["draft", "commitmentExtraction", "sceneAssessment"] {
                let id = format!("{kind}-{chapter}");
                db.execute("INSERT INTO story_work_steps_v1 VALUES('work',?1)", [&id])
                    .unwrap();
                db.execute(
                    "INSERT INTO story_tasks_v2 VALUES(?1,'project','completed')",
                    [&id],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO story_task_outputs_v2 VALUES(?1,?1,?2,'chapterComplete')",
                    params![id, kind],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO story_object_heads_v2 VALUES('project',?1,NULL)",
                    [&id],
                )
                .unwrap();
                evidence.push(json!({"taskId":id,"revisionId":id}));
            }
        }
        review["evidence"] = json!(evidence);
        let mut incomplete = review.clone();
        incomplete["evidence"].as_array_mut().unwrap().pop();
        assert!(save(&db, &task, "work", &status(), Some(&incomplete)).is_err());
        save(&db, &task, "work", &status(), Some(&review)).unwrap();
        let stored: String = db
            .query_row(
                "SELECT content_json FROM story_work_delivery_reviews_v1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored).unwrap()["evidence"]
                .as_array()
                .unwrap()
                .len(),
            300
        );
    }
}
