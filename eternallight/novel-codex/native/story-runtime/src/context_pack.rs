use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub(crate) struct ContextPackInput {
    pub query_report: Option<Value>,
    pub query_scope_policy: Option<Value>,
    pub project_id: String,
    pub project_revision: i64,
    pub snapshot_id: String,
    pub task_id: Option<String>,
    pub purpose: String,
    pub actor: String,
    pub position: Value,
    pub task_target: Value,
    pub delegation_targets: Option<Value>,
    pub input_manifest: Option<Value>,
    pub title: String,
    pub logline: String,
    pub artifacts: Map<String, Value>,
    pub drafts: Vec<Value>,
    pub facts: Vec<Value>,
    pub modeling: Vec<Value>,
    pub commitments: Value,
    pub planning_review: Option<Value>,
    pub modeling_batch: Option<Value>,
    pub entity_catalog: Option<Value>,
    pub working_knowledge: Option<Value>,
    pub scene_plan_draft_links: Vec<Value>,
    pub scene_links_truncated: bool,
    pub scene_event_refs_truncated: bool,
    pub source_watermarks: Value,
    pub chapters_in_scope: usize,
    pub modeling_sources_in_scope: usize,
    pub drafts_truncated: bool,
    pub facts_truncated: bool,
    pub typed_state_facts_withheld: usize,
    pub typed_event_facts_withheld: usize,
    pub modeling_complete: bool,
    pub working_knowledge_complete: bool,
    pub pov_evidence_withheld: bool,
    pub pov_prior_text_withheld: bool,
    pub byte_limit: usize,
    pub entry_limit: usize,
}

pub(crate) fn build_context_pack(input: ContextPackInput) -> Value {
    let mut missing_scope = Vec::new();
    let mut truncation_reasons = Vec::new();
    if input.commitments["coverage"]["complete"] != true {
        missing_scope.push("narrativeCommitments".to_owned());
    }
    if input.commitments["coverage"]["truncated"] == true {
        truncation_reasons.push("commitmentSelectionBudget".to_owned());
    }
    if input.commitments["proposalContext"]["coverage"]["requested"] == true
        && input.commitments["proposalContext"]["coverage"]["complete"] != true
    {
        missing_scope.push("commitmentProposals".to_owned());
        truncation_reasons.push("proposalSelectionBudget".to_owned());
    }
    if input.drafts_truncated {
        missing_scope.push("chapterText".to_owned());
        truncation_reasons.push("chapterTextBudget".to_owned());
    }
    if input.facts_truncated {
        missing_scope.push("confirmedFacts".to_owned());
        truncation_reasons.push("factBudget".to_owned());
    }
    if input.typed_state_facts_withheld > 0 {
        missing_scope.push("typedStateFacts".to_owned());
    }
    if input.typed_event_facts_withheld > 0 {
        missing_scope.push("typedEventFacts".to_owned());
    }
    if !input.modeling_complete {
        missing_scope.push("knowledgeModeling".to_owned());
    }
    if !input.working_knowledge_complete {
        missing_scope.push("workingKnowledge".to_owned());
    }
    if input
        .entity_catalog
        .as_ref()
        .is_some_and(|catalog| catalog["truncated"] == true)
    {
        missing_scope.push("entityCatalog".to_owned());
        truncation_reasons.push("entityCatalogBudget".to_owned());
    }
    if input
        .query_report
        .as_ref()
        .is_some_and(|report| report["coverage"]["complete"] != true)
    {
        missing_scope.push("queryRetrieval".to_owned());
    }
    if input.pov_evidence_withheld {
        missing_scope.push("povEvidence".to_owned());
    }
    if input.pov_prior_text_withheld {
        missing_scope.push("povPriorText".to_owned());
    }
    if input.scene_links_truncated {
        missing_scope.push("scenePlanDraftLinks".to_owned());
        truncation_reasons.push("sceneLinkEntryLimit".to_owned());
    }
    if input.scene_event_refs_truncated {
        missing_scope.push("sceneEventRefs".to_owned());
        truncation_reasons.push("sceneEventRefLimit".to_owned());
    }
    let active_intents = input
        .input_manifest
        .as_ref()
        .and_then(|manifest| manifest["authorIntent"].as_str())
        .map(|intent| {
            vec![json!({"source": "authorTask", "taskId": input.task_id, "text": intent})]
        })
        .unwrap_or_default();
    let evidence_refs = input
        .facts
        .iter()
        .filter_map(|fact| {
            let id = fact["id"].as_str()?;
            Some(json!({
                "factId": id, "sourceRevisionId": fact.get("sourceRevisionId"),
                "quote": fact.get("evidence"), "confirmationState": "confirmed"
            }))
        })
        .collect::<Vec<_>>();
    let planned_effects = input
        .artifacts
        .get("plan")
        .filter(|value| !value.is_null())
        .map(|plan| vec![json!({"source": "plan", "value": plan})])
        .unwrap_or_default();
    let policy = json!({
        "actor": input.actor, "purpose": input.purpose,
        "taskId": input.task_id, "target": input.task_target,
        "position": input.position,
        "queryScope": input.query_scope_policy,
        "queryPlan": input.query_report.as_ref().map(|report| &report["plan"])
    });
    let policy_digest = digest(&policy);
    let mut pack = json!({
        "id": format!("context-{}", "0".repeat(64)),
        "snapshotId": input.snapshot_id,
        "taskId": input.task_id,
        "purpose": input.purpose,
        "policyDigest": policy_digest,
        "target": {
            "projectId": input.project_id,
            "projectRevision": input.project_revision,
            "position": input.position,
            "operation": input.task_target
        },
        "activeIntentRefs": active_intents,
        "creativeCapabilityBinding": input.input_manifest.as_ref().and_then(|manifest| manifest.get("creativeCapabilityBinding")),
        "hardConstraints": [],
        "narrativeGoals": {
            "title": input.title, "logline": input.logline,
            "brief": input.artifacts.get("brief"),
            "outline": input.artifacts.get("outline")
        },
        "relevantCommitments": input.commitments["entries"],
        "candidateCommitmentProposals": input.commitments["proposalContext"]["entries"],
        "sceneOutcomeRefs": [],
        "plannedEffects": planned_effects,
        "stateFacts": input.facts,
        "evidenceRefs": evidence_refs,
        "relevantText": input.drafts,
        "openQuestions": [],
        "operationContext": {
            "commitmentCoverage": input.commitments["coverage"],
            "commitmentProposalCoverage": input.commitments["proposalContext"]["coverage"],
            "boundCommitmentInputs": input.commitments["assessmentContext"],
            "modeling": input.modeling,
            "planningReview": input.planning_review,
            "modelingBatch": input.modeling_batch,
            "entityCatalog": input.entity_catalog,
            "workingKnowledge": input.working_knowledge,
            "scenePlanDraftLinks": input.scene_plan_draft_links
        },
        "queryReport": input.query_report,
        "queryScope": input.query_scope_policy,
        "coverage": {
            "complete": missing_scope.is_empty(),
            "requestedScope": {
                "purpose": input.purpose,
                "position": input.position,
                "entryLimit": input.entry_limit
            },
            "scannedScope": {
                "chaptersInScope": input.chapters_in_scope,
                "draftsIncluded": input.drafts.len(),
                "factsIncluded": input.facts.len(),
                "typedStateFactsWithheld": input.typed_state_facts_withheld,
                "typedEventFactsWithheld": input.typed_event_facts_withheld,
                "modelingSourcesInScope": input.modeling_sources_in_scope,
                "modelingSourcesIncluded": input.modeling.len()
            },
            "missingScope": missing_scope,
            "truncationReasons": truncation_reasons,
            "semanticCompleteness": "unknown"
        },
        "sourceWatermarks": input.source_watermarks,
        "budgetsUsed": {"byteLimit": input.byte_limit, "byteSize": 0, "entryLimit": input.entry_limit},
        "contentHash": "0".repeat(64)
    });
    if let Some(targets) = input.delegation_targets {
        pack["operationContext"]["delegationTargets"] = targets;
    }
    let mut scene_links_truncated = false;
    if let Some(baseline) = input.artifacts.get("propertyRules") {
        pack["operationContext"]["propertyRulesBase"] = baseline.clone();
    }
    loop {
        pack["sceneOutcomeRefs"] = json!(
            pack["operationContext"]["scenePlanDraftLinks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|link| link["assessmentStatus"] == "assessed")
                .map(|link| json!({
                    "chapterNumber": link["chapterNumber"],
                    "sceneId": link["sceneId"],
                    "assessmentRevisionId": link["sceneOutcomeAssessment"]["revisionId"],
                    "verdict": link["sceneOutcomeAssessment"]["verdict"],
                    "recommendation": link["sceneOutcomeAssessment"]["recommendation"],
                    "authority": "candidateAnalysis"
                }))
                .collect::<Vec<_>>()
        );
        if input.task_id.is_some()
            || pack.to_string().len() + 200 <= input.byte_limit
            || pack["operationContext"]["boundCommitmentInputs"]["scope"] == "frozenPlanReferences"
        {
            break;
        }
        let Some(links) = pack["operationContext"]["scenePlanDraftLinks"].as_array_mut() else {
            break;
        };
        if links.pop().is_none() {
            break;
        }
        scene_links_truncated = true;
    }
    if scene_links_truncated {
        pack["coverage"]["complete"] = json!(false);
        pack["coverage"]["missingScope"]
            .as_array_mut()
            .expect("missing scope array")
            .push(json!("scenePlanDraftLinks"));
        pack["coverage"]["truncationReasons"]
            .as_array_mut()
            .expect("truncation reasons array")
            .push(json!("sceneLinkBudget"));
    }
    // Bound tasks cache complete visible sections before applying transport budgets.
    if input.task_id.is_none() {
        crate::context_budget::fit_optional_context(&mut pack, input.byte_limit);
    }
    loop {
        let size = pack.to_string().len();
        if pack["budgetsUsed"]["byteSize"] == json!(size) {
            break;
        }
        pack["budgetsUsed"]["byteSize"] = json!(size);
    }
    let mut content = pack.clone();
    content
        .as_object_mut()
        .expect("context pack object")
        .remove("id");
    content
        .as_object_mut()
        .expect("context pack object")
        .remove("contentHash");
    let hash = digest(&content);
    pack["id"] = json!(format!("context-{hash}"));
    pack["contentHash"] = json!(hash);
    pack
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
