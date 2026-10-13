//! Short discovery descriptions and on-demand detail share the actual tool spec.
use serde_json::{Value, json};
pub(super) fn spec() -> Value {
    json!({"name":"read_tool_guide","description":"Read detailed instructions for one currently admitted tool. Guidance grants no additional authority.","inputSchema":{"type":"object","additionalProperties":false,"required":["threadId","toolName"],"properties":{"threadId":{"type":"string","minLength":1,"maxLength":160},"toolName":{"type":"string","minLength":1,"maxLength":100},"field":{"type":"string","minLength":1,"maxLength":100,"description":"Optional top-level argument name. Read only its guidance; actual constraints remain in the loaded tool schema."}}}})
}
pub(super) fn summary(name: &str) -> &'static str {
    match name {
        "read_context" => {
            "Read the current task/version context and source directory before source reads. Reuse this directory until its source version changes; previous-task sourceIds/cursors are invalid. Omit byteBudget unless necessary. Directory entries are not content reads."
        }
        "read_source_content" => {
            "Batch frozen visible chapter excerpts with view=passages without sourceId, edge=start/end, characters=1..512 (partial original prose, not complete reading). Find chapters/revisions with view=workspace and chapterNumber/objectKind/revisionId (task bound; no sourceId). Other views require this task's sourceId: content (default parsed fields), raw (JSON), index, locate (exact quote required), scene (groupId required; expands full visible scene). Follow read/nextRead. Prose editAnchor gives immutable coordinates, not write authority. Listings are not reading evidence."
        }
        "save_draft_checkpoint" => {
            "Save an immutable chapter candidate: choose exactly one full payload, exact local patch, or delegatedCandidateId. Preserve task scope. Omit expectedCandidateRevisionId to bind the task-owned CAS; explicit values remain strict assertions. After a committed receipt, complete_task."
        }
        "read_tool_guide" => {
            "Read concise instructions for an admitted tool, optionally one argument field. Tool schemas are loaded through capability discovery; guides do not grant authority."
        }
        "discover_creative_capabilities" => {
            "Find installed tools, optional methods and role templates. Search with query OR read a roleId; never both. Only available tools are callable."
        }
        "read_creative_strategy" => {
            "Read an optional creative method or its catalog. Methods are advice, not story evidence or permission."
        }
        "read_work_status" => {
            "Read current author requirements, ready actions, saved receipts and delivery status; conversationRuns lists same-conversation generation runs. chapterAcceptanceSource is a reference: read its full standards with read_source_content when needed; status pages do not repeat them. verifyChapter.currentDraftSource.read directly locates its saved draft; read it fully before verification."
        }
        "read_work_issues" => {
            "Read paginated persistent issues and observations. Preserve their identities when recording rechecks."
        }
        "read_work_plan" => {
            "Read the current Work plan and its version; a plan is not execution or completed delivery."
        }
        "update_work_plan" => {
            "Update the model's plan within the authorized Work using its current version. Does not execute planned actions."
        }
        "request_work_action" => {
            "Register an action in an existing authorized Work using exactly one ready actionKey or a scoped proposal. Use the returned decisionStatus (the current read_work_status first page) for the next action or completion; no status-only round trip is needed unless state changes or more pages are needed. Copy current version/source digest. read_work_status.chapterAcceptanceSource points to the full frozen standards; read them through read_source_content. For verifyChapter, copy readyActions[].currentDraftSource.read to read the exact current draft directly; follow nextRead to completion. This locator is not reading evidence. This root task MUST fully read its CURRENT authorRequirementsSource AND current draft. Earlier-task reads, directory metadata and summaries are insufficient. Read the guide for chapter checks."
        }
        "request_candidate_task" => {
            "Propose a scoped candidate task from an ordinary advisory. For generateChapter, target must include the existing chapterId and positive chapterNumber; read the guide for chapter acceptance fields. Work decisions use request_work_action."
        }
        "request_generation_handoff" => {
            "Propose new chapter scope or select an existing resumeRun. Receipt registers a handoff; it does not write chapters or approve new scope."
        }
        "pause_generation_run" => {
            "Pause an existing same-conversation run with its current version and author intent revision. Report paused only from the committed receipt."
        }
        "complete_task" => {
            "Complete after persisted output or a registered action. Before a required delivery review, read this task’s authorRequirementsSource fully; read_context exposes rootReadComplete, and source-page coverage shows remaining pages. Include deliveryReview ONLY when the latest decisionStatus or read_work_status has deliveryReviewRequired=true; otherwise omit it and give summary only. For required reviews, put expectedWorkVersion and expectedSourceDigest INSIDE deliveryReview, copied from current status; never at the root. Omit evidence for Native to bind all current saved outputs; an explicit list is a strict complete-list assertion. fulfilled needs unresolved=[] and authorDecisions=[]; optional future adoption/editing is not a blocker."
        }
        "save_brief" => {
            "Save the task's brief candidate with project revision, candidate CAS and idempotency key."
        }
        "save_outline" => {
            "Save the task's outline candidate. Preserve its typed node structure and candidate CAS."
        }
        "save_chapter_plans" => {
            "Save chapter-plan candidates with exact scene identities and supported commitment links under the task's frozen scope."
        }
        "save_property_rules" => {
            "Save the task's property-rule candidate with candidate CAS; candidate rules are not confirmed Canon."
        }
        "save_delegated_draft" => {
            "Save an authorized child's isolated draft candidate. Only the parent may integrate it into a formal checkpoint."
        }
        "read_delegated_draft" => {
            "Read an isolated delegated candidate belonging to this task; it is not a formal checkpoint or adopted draft."
        }
        "query_story" => {
            "Execute a finite typed story query within task visibility. Resolve ambiguity and read original evidence; incomplete results do not prove absence."
        }
        "search_story" => {
            "Find visible story evidence by literal query and filters. Follow pagination and resolve returned evidence references before relying on them."
        }
        "search_relations" => {
            "Search visible explicit relations with bounded hops. Relationship metadata does not establish unstated story facts."
        }
        "resolve_story_evidence" => {
            "Read exact source text for a returned evidence reference. Preserve immutable source identity and Unicode ranges."
        }
        "analyze_story_impact" => {
            "Inspect bounded explicit dependencies of a proposed change; coverage is not a complete semantic impact assessment."
        }
        "simulate_story_hypothesis" => {
            "Explore a bounded hypothetical story change without writing facts or confirming its conclusions."
        }
        "read_commitment" => {
            "Read the task-visible commitment and its exact revision before proposing or revising its candidate."
        }
        "submit_commitment_candidate" => {
            "Save the task-bound commitment revision candidate. Preserve original identity and extraction provenance."
        }
        "submit_commitment_extraction" => {
            "Submit candidate commitments grounded in the exact saved prose and source evidence; extraction does not confirm Canon."
        }
        "publish_knowledge_batch" => {
            "Publish a task-bound modeled knowledge batch with explicit source provenance. After commitment, complete_task; publication closes other tools."
        }
        "read_reading_progress" => {
            "Read durable root-task reading coverage. Delivered ranges do not certify understanding or unseen material."
        }
        "checkpoint_reading" => {
            "Persist a reading checkpoint tied to actual delivered sources and coverage. Child reads cannot certify parent reading."
        }
        "read_revision_outcome" => {
            "Locate the persisted assessment for a writer task. Read its returned source; report existence alone does not mean the revision passes."
        }
        "submit_revision_outcome" => {
            "Save a task-bound revision assessment with exact original/changed prose evidence and unresolved criteria."
        }
        "submit_scene_assessment" => {
            "Save a task-bound scene assessment grounded in actual draft evidence; planned effects are not realized effects."
        }
        "read_chapter_structure_source" => {
            "Read the exact frozen chapter-structure source in bounded Unicode ranges. Missing sources are unavailable, never replaced by latest text."
        }
        "request_planning_review" => {
            "Request an admitted planning review of the exact candidate versions; the request is not a passing review."
        }
        "submit_planning_review_report" => {
            "Save this review seat's report with evidence from its frozen planning packet."
        }
        "read_quality_notebook" => {
            "Read scoped quality observations and proposed method notes. Notes do not become author requirements automatically."
        }
        "record_quality_note" => {
            "Record an evidence-based scoped quality note or proposed method. It grants no tool, budget or author preference."
        }
        "read_quality_review" => {
            "Read a persisted quality review with its actual scope and provenance; a report is not automatic acceptance."
        }
        "read_quality_packet" => {
            "Read the independent reader's authorized packet. Packet visibility cannot be widened by choosing arbitrary sources."
        }
        "submit_quality_reading" => {
            "Save this independent reader's packet-bound report; disclose evidence limits and remaining uncertainty."
        }
        "read_media_workspace" => {
            "Read admitted media documents, versions and available capabilities within this task's workspace."
        }
        "read_media_source" => {
            "Read an authorized immutable media source with its returned cursor and original provenance."
        }
        "read_adaptation_sources" => "List exact saved sources admitted to this task for a new comic/animation adaptation. Follow nextOffset; directory metadata is not original-content reading.",
        "request_media_adaptation" => "Register an author-requested comic/animation adaptation from real saved sources in this same Work. Complete the current task for Host dispatch; registration is not artwork or new budget.",
        "save_media_document" => {
            "Save a media document candidate under its current version and task scope; follow the declared document schema."
        }
        "patch_media_document" => {
            "Apply typed local edits to the exact media-document version under the same authority and CAS as a full save."
        }
        "request_media_generation" => {
            "Register an authorized media-generation job using typed input and approved resources; registration is not completion."
        }
        "read_media_generation" => {
            "Read the actual status and outputs of an admitted media-generation job."
        }
        "wait_media_generation" => {
            "Wait within the bounded timeout for an admitted media-generation job to change state."
        }
        "cancel_media_generation" => {
            "Cancel an admitted media-generation job; report cancellation only from its actual receipt."
        }
        _ => panic!("Missing concise tool description: {name}"),
    }
}
pub(super) fn concise(mut tool: Value) -> Value {
    // Keep the worker-specific contract after operation_tools removes review authority.
    if tool["name"] == "complete_task"
        && tool["inputSchema"]["properties"]
            .get("deliveryReview")
            .is_none()
    {
        return tool;
    }
    tool["description"] = summary(tool["name"].as_str().unwrap()).into();
    if tool["name"] == "read_context"
        && let Some(plan) = tool.pointer_mut("/inputSchema/properties/queryPlan")
    {
        // The full task-specific contract remains in the admitted tool used by
        // read_tool_guide; only this optional field's default prose is deferred.
        plan["description"] = json!(
            "Optional scoped query; omit for ordinary context reading. Before constructing an unfamiliar plan, read_tool_guide(toolName=read_context, field=queryPlan) gives this task's complete query syntax and limits. Existing trusted plans may be copied unchanged."
        );
    }
    tool
}

/// Guides explain intent; the admitted tool schema remains the validation authority.
pub(super) fn read(tools: Vec<Value>, name: &str, field: Option<&str>) -> Result<Value, String> {
    let tool = tools
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or_else(|| guide_error("/toolName", "Choose a tool in the currently admitted set"))?;
    let tool = super::tool_binding::model_tool(tool);
    let schema = &tool["inputSchema"];
    let properties = schema["properties"]
        .as_object()
        .ok_or("Tool has no argument schema")?;
    let mut result = json!({"toolName":name,"authority":"guidanceOnly",
        "instructions":tool["description"],"required":schema["required"],
        "fields":properties.keys().collect::<Vec<_>>(),
        "schemaSource":"Use the current loaded tool schema for complete constraints; discover the tool if not loaded."});
    if let Some(field) = field {
        let argument = properties.get(field).ok_or_else(|| {
            guide_error(
                "/field",
                "Choose an exact top-level argument from this tool's fields",
            )
        })?;
        result["field"] = json!(field);
        result["fieldGuidance"] = field_guidance(argument);
    }
    Ok(result)
}

fn guide_error(path: &str, expected: &str) -> String {
    json!({"code":"invalid_tool_guide","path":path,"expected":expected}).to_string()
}

fn field_guidance(schema: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "description",
        "type",
        "enum",
        "const",
        "required",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
    ] {
        if let Some(value) = schema.get(key) {
            result.insert(key.into(), value.clone());
        }
    }
    if let Some(properties) = schema["properties"].as_object() {
        result.insert(
            "fields".into(),
            json!(properties.keys().collect::<Vec<_>>()),
        );
    }
    if let Some(item) = schema.get("items") {
        result.insert("itemGuidance".into(), field_guidance(item));
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(branches) = schema[key].as_array() {
            result.insert(
                key.into(),
                Value::Array(branches.iter().map(field_guidance).collect()),
            );
        }
    }
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_schema_fits_core_budget_and_keeps_the_complete_payload_contract() {
        let original = super::super::tool_specs().into_iter()
            .find(|tool| tool["name"] == "save_draft_checkpoint").unwrap();
        let tool = concise(super::super::tool_binding::model_tool(original));
        let schema = &tool["inputSchema"];
        let bytes = serde_json::to_vec(schema).unwrap().len();
        assert!(bytes <= 5000, "Core rejects checkpoint schema above 5000 bytes: {bytes}");
        assert!(schema["properties"]["payload"]["description"].as_str().unwrap()
            .contains(&super::super::draft_schema::payload().to_string()));
        assert_eq!(schema["properties"]["patch"], super::super::draft_schema::patch());
        assert_eq!(schema["oneOf"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn quality_reader_guides_cannot_expose_unadmitted_source_or_write_tools() {
        let admitted: Vec<_> = super::super::tool_specs().into_iter()
            .filter(|tool| super::super::allowed_tools("reviewQuality")
                .contains(&tool["name"].as_str().unwrap())).collect();
        assert!(read(admitted.clone(), "read_quality_packet", None).is_ok());
        for forbidden in ["read_context", "read_source_content", "save_draft_checkpoint"] {
            assert!(read(admitted.clone(), forbidden, None).is_err());
        }
    }

    #[test]
    fn guides_do_not_retransmit_schemas_or_require_pagination() {
        let original = super::super::tool_specs();
        let compact: Vec<_> = original
            .iter()
            .cloned()
            .map(super::super::tool_binding::model_tool)
            .map(concise)
            .collect();
        let descriptions = |tools: &[Value]| {
            tools
                .iter()
                .map(|t| t["description"].as_str().unwrap().len())
                .sum::<usize>()
        };
        assert!(descriptions(&compact) * 2 < descriptions(&original));
        for tool in &original {
            let name = tool["name"].as_str().unwrap();
            let guide = read(original.clone(), name, None).unwrap();
            assert_eq!(guide["instructions"], tool["description"]);
            assert!(guide.get("inputSchema").is_none());
            assert!(guide.get("nextRead").is_none());
            assert!(
                !guide["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f == "projectId" || f == "taskId" || f == "threadId")
            );
        }
        let guide = read(original.clone(), "request_work_action", None).unwrap();
        let full = original
            .iter()
            .find(|t| t["name"] == "request_work_action")
            .unwrap()
            .to_string();
        assert!(guide.to_string().len() * 2 < full.len());
        let guide = read(original.clone(), "save_draft_checkpoint", Some("patch")).unwrap();
        assert!(
            guide["fieldGuidance"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f == "sourceRevisionId")
        );
        assert!(read(original.clone(), "save_draft_checkpoint", Some("projectId")).is_err());
        assert!(read(vec![], "save_draft_checkpoint", None).is_err());
        assert!(read(original, "save_draft_checkpoint", Some("invented")).is_err());
    }
}
