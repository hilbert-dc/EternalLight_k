use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{CreateTaskRequest, RepositoryError, TaskRecord};

struct MethodSpec {
    id: &'static str,
    text: &'static str,
    operations: &'static [&'static str],
    context: &'static [&'static str],
}

const WORKFLOW: MethodSpec = MethodSpec {
    id: "novel-workflow",
    text: include_str!("../../../codex-runtime/skills/novel-workflow/SKILL.md"),
    operations: &[],
    context: &["taskTarget", "sourceWatermarks", "coverage"],
};
const METHODS: [MethodSpec; 3] = [
    MethodSpec {
        id: "novel-canon",
        text: include_str!("../../../codex-runtime/skills/novel-canon/SKILL.md"),
        operations: &["advisory", "modelKnowledge", "generateChapter"],
        context: &["sourceEvidence", "workingKnowledge"],
    },
    MethodSpec {
        id: "narrative-ir",
        text: include_str!("../../../codex-runtime/skills/narrative-ir/SKILL.md"),
        operations: &["generateOutline", "generatePlan", "generateChapter"],
        context: &["plannedEffects", "relevantText"],
    },
    MethodSpec {
        id: "story-tests",
        text: include_str!("../../../codex-runtime/skills/story-tests/SKILL.md"),
        operations: &["advisory", "reviewPlanningSeat", "assessSceneOutcome"],
        context: &["sourceEvidence", "coverage"],
    },
];

fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn method(spec: &MethodSpec, operation: &str, status: &str, kind: &str) -> Value {
    json!({
        "id":spec.id, "version":"1.0.0", "digest":digest(spec.text.as_bytes()),
        "status":status, "kind":kind, "phase":operation,
        "reason":if kind=="executionContract" {"mandatoryTaskContract"} else {"authorExplicitExperiment"},
        "requiredContextKinds":spec.context, "outputSchemaRef":format!("task-output/{operation}"),
        "limitations":["candidateOnly", "noLiteraryQualityCertification", "noPermissionExpansion"],
        "sourceAttribution":format!("product/codex-runtime/skills/{}/SKILL.md",spec.id),
        "text":spec.text
    })
}

pub(crate) fn freeze(
    db: &Connection,
    request: &CreateTaskRequest,
    target: &Value,
    manifest: &mut Value,
) -> Result<(), RepositoryError> {
    if manifest.get("creativeCapabilityBinding").is_some() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    if manifest.get("experimentalMethods").is_none()
        && let Some(step) = target["resumeStepId"].as_str()
    {
        let previous: Option<String> = db.query_row(
            "SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id JOIN story_work_tasks_v1 w ON w.id=s.work_task_id WHERE s.id=?1 AND w.project_id=?2 AND s.operation=?3 ORDER BY a.attempt_number DESC LIMIT 1",
            params![step,request.project_id.as_str(),request.operation], |row|row.get(0),
        ).optional().map_err(|error|RepositoryError::Storage(error.to_string()))?;
        if let Some(previous) = previous {
            let prior = crate::tasks::load_task(db, &previous)?;
            let prior: Value = serde_json::from_str(&prior.input_manifest_json)
                .map_err(|_| RepositoryError::CorruptState)?;
            manifest["experimentalMethods"] = prior
                .get("experimentalMethods")
                .cloned()
                .unwrap_or_else(|| json!([]));
        }
    }
    let mut selected = vec![method(
        &WORKFLOW,
        &request.operation,
        "stable",
        "executionContract",
    )];
    let mut chosen = HashSet::new();
    if let Some(experimental) = manifest.get("experimentalMethods") {
        let ids = experimental
            .as_array()
            .filter(|ids| ids.len() <= 2)
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        for id in ids {
            let id = id.as_str().ok_or(RepositoryError::TaskBindingInvalid)?;
            let spec = METHODS
                .iter()
                .find(|spec| spec.id == id && spec.operations.contains(&request.operation.as_str()))
                .ok_or(RepositoryError::TaskBindingInvalid)?;
            if !chosen.insert(id) {
                return Err(RepositoryError::TaskBindingInvalid);
            }
            selected.push(method(
                spec,
                &request.operation,
                "candidate",
                "creativeMethod",
            ));
        }
    }
    if selected
        .iter()
        .any(|item| item["text"].as_str().is_none_or(|text| text.len() > 12_000))
        || selected
            .iter()
            .map(|item| item["text"].as_str().map_or(0, str::len))
            .sum::<usize>()
            > 20_000
    {
        return Err(RepositoryError::InvalidContent(
            "creative method budget exceeded".into(),
        ));
    }
    let snapshot: String = db.query_row(
        "SELECT head_snapshot_id FROM story_branches_v2 WHERE project_id=?1 AND branch_id='main'",
        params![request.project_id.as_str()], |row| row.get(0),
    ).map_err(|error| RepositoryError::Storage(error.to_string()))?;
    let mut binding = json!({
        "taskId":request.task_id, "projectId":request.project_id.as_str(),
        "snapshotId":snapshot, "inputRevision":request.input_revision, "operation":request.operation,
        "intentGeneration":target.get("intentGeneration"), "targetDigest":digest(target.to_string().as_bytes()),
        "routingPolicyDigest":digest(b"creative-methods-v1:mandatory-contract:author-experiment-only:max-two"),
        "executionMode":"engineManaged", "grantedToolScope":"existingOperationContract",
        "selectedMethods":selected,
        "omittedMethods":METHODS.iter().filter(|spec| !chosen.contains(spec.id))
            .map(|spec|json!({"id":spec.id,"reason":"notRequestedOrUnsupported"})).collect::<Vec<_>>()
    });
    binding["bindingDigest"] = json!(digest(binding.to_string().as_bytes()));
    manifest["creativeCapabilityBinding"] = binding;
    Ok(())
}

/// Verify persisted content, not the current registry: a later binary must not reroute old tasks.
pub(crate) fn validate(task: &TaskRecord) -> Result<(), RepositoryError> {
    let manifest: Value = serde_json::from_str(&task.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    let mut binding = manifest["creativeCapabilityBinding"].clone();
    let target: Value =
        serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
    if binding["taskId"] != task.task_id
        || binding["projectId"] != task.project_id.as_str()
        || binding["operation"] != task.operation
        || binding["inputRevision"] != task.input_revision
        || binding["targetDigest"] != digest(target.to_string().as_bytes())
    {
        return Err(RepositoryError::CorruptState);
    }
    let expected = binding
        .as_object_mut()
        .and_then(|value| value.remove("bindingDigest"))
        .ok_or(RepositoryError::CorruptState)?;
    if expected != digest(binding.to_string().as_bytes()) {
        return Err(RepositoryError::CorruptState);
    }
    let selected = binding["selectedMethods"]
        .as_array()
        .filter(|methods| (1..=3).contains(&methods.len()))
        .ok_or(RepositoryError::CorruptState)?;
    if selected[0]["id"] != "novel-workflow"
        || selected[0]["kind"] != "executionContract"
        || selected
            .iter()
            .map(|item| item["text"].as_str().map_or(0, str::len))
            .sum::<usize>()
            > 20_000
    {
        return Err(RepositoryError::CorruptState);
    }
    for method in selected {
        let text = method["text"]
            .as_str()
            .filter(|text| text.len() <= 12_000)
            .ok_or(RepositoryError::CorruptState)?;
        if method["digest"] != digest(text.as_bytes()) {
            return Err(RepositoryError::CorruptState);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "creative_methods_tests.rs"]
mod tests;
