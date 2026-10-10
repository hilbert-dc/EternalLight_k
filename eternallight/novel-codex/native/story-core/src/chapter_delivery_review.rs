//! Model-owned acceptance, bound to a current saved draft. Saving remains unconditional
//! on literary quality; advancing to the next chapter requires an explicit assessment.
use crate::{ProjectId, RepositoryError, authoring_runs::storage_error};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Review {
    source_revision_id: String,
    assessment_revision_id: String,
    checks: Vec<Check>,
    length: Length,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Check {
    #[serde(rename = "criterionId")]
    criterion_id: String,
    #[serde(rename = "issueId")]
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    issue_id: Option<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Length {
    requirement: String,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    min_characters: Option<usize>,
    #[serde(deserialize_with = "crate::deserialize_execution_limit")]
    max_characters: Option<usize>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Acceptance {
    pub(crate) requirements: Vec<Criterion>,
    pub(crate) length: Length,
    pub(crate) interpretation_correction: Option<InterpretationCorrection>,
    verification: Option<Verification>,
}
#[derive(PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Verification {
    method: String,
    reason: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Criterion {
    pub(crate) id: String,
    pub(crate) text: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InterpretationCorrection {
    reason: String,
    impact: String,
    source_checks: Vec<AuthorRequirementQuote>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthorRequirementQuote {
    source_id: String,
    content_hash: String,
    quote: String,
}
fn invalid(message: &str) -> RepositoryError {
    RepositoryError::InvalidContent(format!("chapterReview: {message}"))
}
fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 600
}
fn validate_length(length: &Length) -> Result<(), RepositoryError> {
    if !text(&length.requirement)
        || length
            .min_characters
            .zip(length.max_characters)
            .is_some_and(|(min, max)| min > max)
        || length.min_characters.is_some_and(|min| min > 1_000_000)
        || length.max_characters.is_some_and(|max| max > 1_000_000)
    {
        return Err(invalid("invalid author-derived length range"));
    }
    Ok(())
}

pub(crate) fn validate_acceptance(value: &Value) -> Result<Acceptance, RepositoryError> {
    if value.to_string().len() > 3500 {
        return Err(invalid("keep chapterAcceptance within 3500 UTF-8 bytes"));
    }
    let result: Acceptance = serde_json::from_value(value.clone())
        .map_err(|_| invalid("invalid chapterAcceptance fields"))?;
    validate_length(&result.length)?;
    if result.verification.as_ref().is_some_and(|v| !matches!(v.method.as_str(), "direct" | "independent") || !text(&v.reason)) {
        return Err(invalid("verification requires method direct/independent and a reason based on the author request"));
    }
    if let Some(correction) = &result.interpretation_correction {
        if !text(&correction.reason)
            || !text(&correction.impact)
            || correction.source_checks.is_empty()
            || correction.source_checks.len() > 4
        {
            return Err(invalid(
                "interpretation correction needs reason, impact and actual author-source quotes",
            ));
        }
    }
    if result.requirements.is_empty()
        || result.requirements.len() > 12
        || result
            .requirements
            .iter()
            .any(|s| !crate::work_issues::identifier(&s.id) || !text(&s.text))
        || result
            .requirements
            .iter()
            .map(|s| &s.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != result.requirements.len()
    {
        return Err(invalid("list unique author requirements before drafting"));
    }
    Ok(result)
}
pub(crate) fn length_result(value: &Value, count: usize) -> Result<Value, RepositoryError> {
    let acceptance = validate_acceptance(value)?;
    let passed = acceptance
        .length
        .min_characters
        .is_none_or(|min| count >= min)
        && acceptance
            .length
            .max_characters
            .is_none_or(|max| count <= max);
    Ok(
        json!({"characterCount":count,"minCharacters":acceptance.length.min_characters,"maxCharacters":acceptance.length.max_characters,"satisfied":passed,"countUnit":"Unicode characters excluding whitespace; punctuation included; body only"}),
    )
}

pub(crate) fn same_acceptance(a: &Value, b: &Value) -> Result<bool, RepositoryError> {
    let a = validate_acceptance(a)?;
    let b = validate_acceptance(b)?;
    Ok(a.length == b.length && a.verification == b.verification
        && a.requirements
            .iter()
            .map(|c| (&c.id, &c.text))
            .collect::<std::collections::BTreeSet<_>>()
            == b.requirements
                .iter()
                .map(|c| (&c.id, &c.text))
                .collect::<std::collections::BTreeSet<_>>())
}

// Author source stays immutable. A model interpretation can be corrected with
// actual author evidence. Whether the new interpretation fits the author is a
// model judgment; current draft length must not veto a source-backed correction.
// Correction records do not certify prose or replace independent assessment.
pub(crate) fn bind_acceptance(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    target: &mut Value,
    value: Option<&Value>,
) -> Result<(), RepositoryError> {
    let number = target["chapterNumber"]
        .as_i64()
        .ok_or(RepositoryError::TaskBindingInvalid)?;
    let generation = target["intentGeneration"].as_i64().unwrap_or(0);
    let value = value
        .ok_or_else(|| invalid("writer decision requires chapterAcceptance before drafting"))?;
    if value.to_string().len() > 3500 {
        return Err(invalid(
            "keep chapterAcceptance within 3500 UTF-8 bytes for bounded status reading",
        ));
    }
    let acceptance = validate_acceptance(value)?;
    if target["desiredOutcomes"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|o| acceptance.requirements.iter().any(|c| o["id"] == c.id))
    }) {
        return Err(invalid(
            "chapter criteria and selected scene outcome IDs must not collide",
        ));
    }
    validate_length(&acceptance.length)?;
    if acceptance.requirements.is_empty()
        || acceptance.requirements.len() > 12
        || acceptance
            .requirements
            .iter()
            .any(|s| !crate::work_issues::identifier(&s.id) || !text(&s.text))
        || acceptance
            .requirements
            .iter()
            .map(|s| &s.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != acceptance.requirements.len()
    {
        return Err(invalid("list unique author requirements before drafting"));
    }
    let prior:Option<String>=db.query_row(
        "SELECT target_json FROM story_work_decision_actions_v1 WHERE work_task_id=?1 AND operation IN ('writeNextChapter','generateChapter') AND json_extract(target_json,'$.chapterNumber')=?2 AND COALESCE(json_extract(target_json,'$.intentGeneration'),0)=?3 ORDER BY rowid DESC LIMIT 1",
        params![work,number,generation],|row|row.get(0),
    ).optional().map_err(storage_error)?;
    let prior = prior.or(db.query_row("SELECT a.target_json FROM story_task_action_requests_v1 a JOIN story_work_attempts_v1 p ON p.attempt_id=a.source_task_id JOIN story_work_steps_v1 s ON s.id=p.step_id WHERE s.work_task_id=?1 AND a.operation='generateChapter' AND json_extract(a.target_json,'$.chapterNumber')=?2 ORDER BY a.rowid LIMIT 1", params![work,number], |row|row.get(0)).optional().map_err(storage_error)?);
    if let Some(prior) = prior {
        let prior: Value =
            serde_json::from_str(&prior).map_err(|_| RepositoryError::CorruptState)?;
        let prior_acceptance = validate_acceptance(&prior["chapterAcceptance"])?;
        if prior_acceptance
            .requirements
            .iter()
            .map(|c| (&c.id, &c.text))
            .collect::<std::collections::BTreeSet<_>>()
            != acceptance
                .requirements
                .iter()
                .map(|c| (&c.id, &c.text))
                .collect::<std::collections::BTreeSet<_>>()
            || prior_acceptance.length != acceptance.length
            || prior_acceptance.verification != acceptance.verification
        {
            let correction=acceptance.interpretation_correction.as_ref().ok_or_else(||invalid("correct a model interpretation using interpretationCorrection with original author evidence; never silently replace author requirements"))?;
            for source in &correction.source_checks {
                crate::task_sources::validate_author_requirement_quote(
                    db,
                    project,
                    work,
                    &source.source_id,
                    &source.content_hash,
                    &source.quote,
                )?;
            }

        }
    }
    // project is checked even when there is no prior writer.
    let owned: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_work_tasks_v1 WHERE id=?1 AND project_id=?2 )",
            params![work, project.as_str()],
            |row| row.get(0),
        )
        .map_err(storage_error)?;
    if !owned {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    target["chapterAcceptance"] = value.clone();
    Ok(())
}

pub(crate) fn current_acceptance(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
) -> Result<Value, RepositoryError> {
    let target:Option<String>=db.query_row(
        "SELECT a.target_json FROM story_work_decision_actions_v1 a JOIN story_work_tasks_v1 w ON w.id=a.work_task_id WHERE w.project_id=?1 AND w.kind='generation' AND a.operation='generateChapter' AND json_extract(a.target_json,'$.chapterId')=?2 ORDER BY a.rowid DESC LIMIT 1",
        params![project.as_str(),chapter],|row|row.get(0),
    ).optional().map_err(storage_error)?;
    let target: Value = serde_json::from_str(
        &target.ok_or_else(|| invalid("no frozen writer acceptance for this chapter"))?,
    )
    .map_err(|_| RepositoryError::CorruptState)?;
    if !target["chapterAcceptance"].is_object() {
        return Err(invalid("no frozen writer acceptance for this chapter"));
    }
    Ok(target["chapterAcceptance"].clone())
}
fn validate(value: &Value, revision: &str, count: usize) -> Result<(), RepositoryError> {
    let review: Review =
        serde_json::from_value(value.clone()).map_err(|_| invalid("invalid fields"))?;
    if review.source_revision_id != revision {
        return Err(RepositoryError::TaskInputStale);
    }
    if review.checks.is_empty()
        || review.checks.len() > 12
        || !crate::work_issues::identifier(&review.assessment_revision_id)
        || review.checks.iter().any(|c| {
            !crate::work_issues::identifier(&c.criterion_id)
                || c.issue_id
                    .as_ref()
                    .is_some_and(|id| !crate::work_issues::identifier(id))
        })
        || review
            .checks
            .iter()
            .map(|c| &c.criterion_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != review.checks.len()
        || !text(&review.length.requirement)
    {
        return Err(invalid(
            "reference each frozen criterion in the current assessment report",
        ));
    }
    let length = review.length;
    validate_length(&length)?;
    if length.min_characters.is_some_and(|min| count < min)
        || length.max_characters.is_some_and(|max| count > max)
    {
        return Err(invalid(&format!(
            "saved body has {count} characters, outside the assessed author requirement; revise this chapter, do not defer the requirement"
        )));
    }
    Ok(())
}

pub(crate) fn verify(
    db: &Connection,
    project: &ProjectId,
    chapter: &str,
    revision: &str,
    value: Option<&Value>,
) -> Result<Value, RepositoryError> {
    let review = value.ok_or_else(|| {
        invalid("finishChapter requires an assessment of the current saved draft")
    })?;
    let content: Option<String> = db.query_row(
        "SELECT r.content_json FROM story_object_heads_v2 h JOIN story_revisions_v2 r ON r.project_id=h.project_id AND r.object_id=h.object_id AND r.revision_id=COALESCE(h.candidate_revision_id,h.accepted_revision_id) WHERE h.project_id=?1 AND h.object_id='draft-'||?2 AND h.object_kind='draft' AND r.revision_id=?3",
        params![project.as_str(),chapter,revision], |row| row.get(0),
    ).optional().map_err(storage_error)?;
    let content: Value = serde_json::from_str(&content.ok_or(RepositoryError::TaskInputStale)?)
        .map_err(|_| RepositoryError::CorruptState)?;
    if content["completeness"] != "chapterComplete" || content["chapterId"] != chapter {
        return Err(invalid("a complete saved chapter is required"));
    }
    let body = content["body"]
        .as_str()
        .ok_or(RepositoryError::CorruptState)?;
    let count = crate::work_decision_status::chapter_character_count(body);
    validate(review, revision, count)?;
    let work: String = db.query_row(
        "SELECT a.work_task_id FROM story_work_decision_actions_v1 a JOIN story_work_tasks_v1 w ON w.id=a.work_task_id WHERE w.project_id=?1 AND w.kind='generation' AND a.operation='generateChapter' AND json_extract(a.target_json,'$.chapterId')=?2 ORDER BY a.rowid DESC LIMIT 1",
        params![project.as_str(),chapter], |row| row.get(0),
    ).optional().map_err(storage_error)?.ok_or_else(|| invalid("no frozen writer acceptance for this chapter"))?;
    let independent =
        crate::revision_outcome::verify_chapter(db, project, &work, chapter, revision)?;
    let parsed: Review =
        serde_json::from_value(review.clone()).map_err(|_| invalid("invalid fields"))?;
    if independent["reportRevisionId"] != parsed.assessment_revision_id {
        return Err(invalid(
            "chapterReview must reference the current assessment report",
        ));
    }
    let acceptance = current_acceptance(db, project, chapter)?;
    let requirements: std::collections::BTreeSet<_> = acceptance["requirements"]
        .as_array()
        .ok_or(RepositoryError::CorruptState)?
        .iter()
        .filter_map(|c| c["id"].as_str())
        .collect();
    let checked: std::collections::BTreeSet<_> = parsed
        .checks
        .iter()
        .map(|c| c.criterion_id.as_str())
        .collect();
    if requirements != checked || acceptance["length"] != review["length"] {
        return Err(invalid(
            "review must reference every frozen criterion without dropping requirements",
        ));
    }
    for check in &parsed.checks {
        let actual = independent["checks"]
            .as_array()
            .ok_or(RepositoryError::CorruptState)?
            .iter()
            .find(|c| c["id"] == check.criterion_id)
            .ok_or_else(|| invalid("criterion not in current assessment report"))?;
        if actual["status"] != "supported" || actual["issueId"] != json!(check.issue_id) {
            return Err(invalid(
                "chapter review must reference the actual supported criterion and problem identity",
            ));
        }
    }
    crate::work_issues::require_resolved(db, project, &work, Some(chapter))?;
    Ok(
        json!({"revisionId":revision,"chapterId":chapter,"chapterNumber":content["chapterNumber"],
        "characterCount":count,"countUnit":"Unicode characters excluding whitespace; punctuation included; body only",
        "assessmentRevisionId":independent["reportRevisionId"],"assessmentMethod":independent.get("method").cloned().unwrap_or_else(||json!("independent")),"authority":"persistedDraftAndModelAssessment"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn review() -> Value {
        json!({"sourceRevisionId":"draft-current","assessmentRevisionId":"report-current","checks":[{"criterionId":"deadline","issueId":null}],
            "length":{"requirement":"约三千字，判断可接受范围为2700—3300正文字符","minCharacters":2700,"maxCharacters":3300}})
    }
    #[test]
    fn acceptance_binds_current_prose_and_author_derived_bounds() {
        assert!(validate(&review(), "draft-current", 3000).is_ok());
        assert!(validate(&review(), "draft-current", 1918).is_err());
        assert!(validate(&review(), "draft-current", 3301).is_err());
        assert!(validate(&review(), "other-draft", 3000).is_err());
        for field in ["minCharacters", "maxCharacters"] {
            let mut missing = review();
            missing["length"].as_object_mut().unwrap().remove(field);
            assert!(validate(&missing, "draft-current", 3000).is_err());
        }
        for change in [
            json!([]),
            json!([{"requirement":"期限","assessment":"尚有矛盾","status":"unresolved"}]),
            json!([{"requirement":"期限","assessment":"","status":"satisfied"}]),
        ] {
            let mut value = review();
            value["checks"] = change;
            assert!(validate(&value, "draft-current", 3000).is_err());
        }
        let mut missing_issue = review();
        missing_issue["checks"][0]
            .as_object_mut()
            .unwrap()
            .remove("issueId");
        assert!(validate(&missing_issue, "draft-current", 3000).is_err());
        let mut unresolved = review();
        unresolved["checks"][0]["status"] = json!("unresolved");
        assert!(validate(&unresolved, "draft-current", 3000).is_err());
        let mut prior_only = review();
        prior_only["checks"][0]["sourceChecks"] = json!([{"revisionId":"prior-draft"}]);
        assert!(validate(&prior_only, "draft-current", 3000).is_err());
        let mut short = review();
        short["length"] = json!({"requirement":"作者只要求修改一句，无篇幅限制","minCharacters":null,"maxCharacters":null});
        assert!(validate(&short, "draft-current", 80).is_ok());
        short["length"]["minCharacters"] = json!(100);
        short["length"]["maxCharacters"] = json!(10);
        assert!(validate(&short, "draft-current", 80).is_err());
    }
    #[test]
    fn revisions_cannot_change_frozen_standards_and_reviews_use_saved_unicode_body() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE story_task_action_requests_v1(source_task_id TEXT,operation TEXT,target_json TEXT); CREATE TABLE story_work_attempts_v1(attempt_id TEXT,step_id TEXT); CREATE TABLE story_work_steps_v1(id TEXT,work_task_id TEXT); CREATE TABLE story_work_tasks_v1(id TEXT,project_id TEXT,kind TEXT); CREATE TABLE story_work_decision_actions_v1(work_task_id TEXT,operation TEXT,target_json TEXT); CREATE TABLE story_object_heads_v2(project_id TEXT,object_id TEXT,object_kind TEXT,candidate_revision_id TEXT,accepted_revision_id TEXT); CREATE TABLE story_revisions_v2(project_id TEXT,object_id TEXT,revision_id TEXT,content_json TEXT); INSERT INTO story_work_tasks_v1 VALUES('work','project','generation');").unwrap();
        let project = ProjectId::parse("project").unwrap();
        let acceptance = json!({"requirements":[{"id":"whole-chapter","text":"保留子时期限"}],"length":{"requirement":"至少8个正文字符","minCharacters":8,"maxCharacters":null}});
        let mut target = json!({"chapterNumber":1,"intentGeneration":0,"chapterId":"chapter"});
        bind_acceptance(&db, &project, "work", &mut target, Some(&acceptance)).unwrap();
        db.execute(
            "INSERT INTO story_work_decision_actions_v1 VALUES('work','generateChapter',?1)",
            [target.to_string()],
        )
        .unwrap();
        let mut changed = acceptance.clone();
        changed["length"]["minCharacters"] = json!(1);
        assert!(bind_acceptance(&db, &project, "work", &mut target, Some(&changed)).is_err());
        assert!(bind_acceptance(&db, &project, "work", &mut target, None).is_err());
        assert!(
            bind_acceptance(
                &db,
                &ProjectId::parse("other").unwrap(),
                "work",
                &mut target,
                Some(&acceptance)
            )
            .is_err()
        );
        // Author-intent changes create a new interpretation; they are not revisions
        // pretending to be a different threshold in the same intent generation.
        target["intentGeneration"] = json!(1);
        bind_acceptance(&db, &project, "work", &mut target, Some(&changed)).unwrap();
        let draft = json!({"chapterId":"chapter","chapterNumber":1,"completeness":"chapterComplete","body":"雨夜，灯亮。 A 🕯\u{feff}"});
        db.execute(
            "INSERT INTO story_revisions_v2 VALUES('project','draft-chapter','draft-current',?1)",
            [draft.to_string()],
        )
        .unwrap();
        db.execute("INSERT INTO story_object_heads_v2 VALUES('project','draft-chapter','draft','draft-current',NULL)",[]).unwrap();
        let source_check = crate::source_check_evidence::tests::source_fixture(
            &db,
            "work",
            "project",
            "draft-current",
            draft["body"].as_str().unwrap(),
        );
        let review = json!({"sourceRevisionId":"draft-current","checks":[{"requirement":"保留子时期限","assessment":"已核对原文","status":"satisfied","sourceChecks":[source_check]}],"length":acceptance["length"]});
        assert_eq!(
            length_result(
                &acceptance,
                crate::work_decision_status::chapter_character_count(
                    draft["body"].as_str().unwrap()
                )
            )
            .unwrap()["characterCount"],
            8
        );
        assert!(verify(&db, &project, "chapter", "draft-current", Some(&review)).is_err());
        assert!(verify(&db, &project, "chapter", "draft-current", None).is_err());
        assert!(verify(&db, &project, "chapter", "old-revision", Some(&review)).is_err());
        for checks in [
            json!([]),
            json!([{"sourceId":"unknown","revisionId":"draft-current","contentHash":"a".repeat(64),"evidence":{"quote":"雨夜"}}]),
            json!([{"sourceId":review["checks"][0]["sourceChecks"][0]["sourceId"],"revisionId":"draft-current","contentHash":"a".repeat(64),"evidence":{"quote":"不存在的前情"}}]),
        ] {
            let mut unproven = review.clone();
            unproven["checks"][0]["sourceChecks"] = checks;
            assert!(verify(&db, &project, "chapter", "draft-current", Some(&unproven)).is_err());
        }
        let mut weakened = review.clone();
        weakened["length"]["minCharacters"] = json!(1);
        assert!(verify(&db, &project, "chapter", "draft-current", Some(&weakened)).is_err());
        let mut dropped = review;
        dropped["checks"][0]["requirement"] = json!("忽略期限");
        assert!(verify(&db, &project, "chapter", "draft-current", Some(&dropped)).is_err());
    }
}
