use super::*;

/// Only packets prepared by this Work are autonomous choices. Author-created
/// packets remain explicit manual requests, never ambient spending authority.
pub(crate) fn reader_actions(
    db: &Connection,
    project: &ProjectId,
    work: &str,
    actions: &mut Vec<crate::work_decision_status::ReadyAction>,
) -> Result<(), RepositoryError> {
    let mut query = db.prepare("SELECT p.id,p.sources_json FROM story_quality_packets_v1 p WHERE p.project_id=?1 AND p.task_id IS NULL AND p.report_json IS NULL AND EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=p.created_by_task_id AND s.work_task_id=?2) AND NOT EXISTS(SELECT 1 FROM story_work_steps_v1 s WHERE s.work_task_id=?2 AND s.operation='reviewQuality' AND json_extract(s.target_json,'$.qualityPacketId')=p.id) ORDER BY p.ordinal").map_err(storage)?;
    let rows = query
        .query_map(params![project.as_str(), work], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(storage)?;
    for row in rows {
        let (id, raw) = row.map_err(storage)?;
        let sources: Vec<QualityPacketSource> =
            serde_json::from_str(&raw).map_err(|_| RepositoryError::CorruptState)?;
        let mut current = true;
        for s in sources {
            if let Some(revision) = s.revision_id {
                current &= source(db, project, &revision)?.1;
            }
            if let Some(id) = s.note_id {
                let (n, status) = note(db, project, &id)?;
                current &= status != "withdrawn" && evidence(db, project, &n.evidence)?;
            }
        }
        if current {
            actions.push(crate::work_decision_status::action(
                "reviewQuality",
                json!({"type":"project","qualityPacketId":id}),
                None,
                None,
                false,
            ));
        }
    }
    Ok(())
}

pub(crate) fn bind_reader(
    db: &Connection,
    request: &crate::CreateTaskRequest,
) -> Result<(), RepositoryError> {
    if request.operation != "reviewQuality" {
        return Ok(());
    }
    let target = parse(&request.target_json)?;
    let allowed = [
        "type",
        "qualityPacketId",
        "workDecisionAction",
        "resumeStepId",
        "runId",
        "runVersion",
        "stepToken",
        "intentGeneration",
        "recoveryPlan",
    ];
    if target["type"] != "project"
        || target
            .as_object()
            .is_none_or(|t| t.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(invalid("Reader target contains unsupported fields"));
    }
    // Work metadata is authenticated against the queued step by freeze/attach_attempt,
    // never forwarded into the isolated reader prompt.
    if target.as_object().unwrap().len() > 2 && target.get("workDecisionAction").is_none() {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let id = target["qualityPacketId"]
        .as_str()
        .ok_or_else(|| invalid("Reader packet is required"))?;
    let (prior,report,sources):(Option<String>,Option<String>,String)=db.query_row("SELECT task_id,report_json,sources_json FROM story_quality_packets_v1 WHERE project_id=?1 AND id=?2",params![request.project_id.as_str(),id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?.ok_or_else(||invalid("Unknown reader packet"))?;
    if report.is_some() {
        return Err(invalid("Packet already reviewed"));
    }
    if let Some(prior) = prior {
        let state: String = db
            .query_row(
                "SELECT status FROM story_tasks_v2 WHERE task_id=?1",
                [prior],
                |r| r.get(0),
            )
            .map_err(storage)?;
        if !matches!(state.as_str(), "failed" | "cancelled") {
            return Err(invalid("Reader already owns this packet"));
        }
    }
    let used: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM story_tasks_v2 WHERE thread_id=?1 AND task_id!=?2)",
            params![request.thread_id, request.task_id],
            |r| r.get(0),
        )
        .map_err(storage)?;
    if used {
        return Err(invalid("Reader requires a fresh task thread"));
    }
    let sources: Vec<QualityPacketSource> =
        serde_json::from_str(&sources).map_err(|_| RepositoryError::CorruptState)?;
    for s in sources {
        if let Some(id) = s.revision_id {
            if !source(db, &request.project_id, &id)?.1 {
                return Err(invalid("Reader source is stale"));
            }
        }
        if let Some(id) = s.note_id {
            let (n, status) = note(db, &request.project_id, &id)?;
            if status == "withdrawn" || !evidence(db, &request.project_id, &n.evidence)? {
                return Err(invalid("Reader trial is stale or withdrawn"));
            }
        }
    }
    db.execute("UPDATE story_quality_packets_v1 SET task_id=?1,delivered_pages=0 WHERE project_id=?2 AND id=?3",params![request.task_id,request.project_id.as_str(),id]).map_err(storage)?;
    Ok(())
}
fn bound(
    db: &Connection,
    task: &TaskRecord,
) -> Result<(String, usize, Option<String>), RepositoryError> {
    let fresh = crate::tasks::load_task(db, &task.task_id)?;
    if fresh.project_id != task.project_id
        || fresh.thread_id != task.thread_id
        || !matches!(fresh.status.as_str(), "queued" | "running")
    {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    if task.operation != "reviewQuality" || !matches!(task.status.as_str(), "queued" | "running") {
        return Err(RepositoryError::TaskOutputForbidden);
    }
    db.query_row("SELECT packet_json,delivered_pages,report_json FROM story_quality_packets_v1 WHERE project_id=?1 AND task_id=?2",params![task.project_id.as_str(),task.task_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?.ok_or(RepositoryError::TaskBindingInvalid)
}
fn pages(packet: &Value) -> Vec<Value> {
    let mut pages = Vec::new();
    for item in packet["items"].as_array().unwrap() {
        let mut part = String::new();
        let mut offset = 0;
        for ch in item["text"].as_str().unwrap().chars() {
            if part.len() + ch.len_utf8() > 3000 {
                pages.push(json!({"label":item["label"],"offset":offset,"text":part}));
                offset += part.chars().count();
                part.clear();
            }
            part.push(ch);
        }
        if !part.is_empty() {
            pages.push(json!({"label":item["label"],"offset":offset,"text":part}));
        }
    }
    pages
}
pub(crate) fn complete_reader(db: &Connection, task: &TaskRecord) -> Result<(), RepositoryError> {
    if task.operation == "reviewQuality" && bound(db, task)?.2.is_none() {
        return Err(invalid("Reader report has not been saved"));
    }
    Ok(())
}

impl QualityStore<'_> {
    pub fn read_quality_packet_page(
        &mut self,
        task: &TaskRecord,
        page: usize,
    ) -> Result<Value, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let (raw, delivered, _) = bound(&tx, task)?;
        let packet = parse(&raw)?;
        let pages = pages(&packet);
        if page > delivered || page >= pages.len() {
            return Err(invalid(
                "Read each packet page in order; do not skip unread text",
            ));
        }
        tx.execute("UPDATE story_quality_packets_v1 SET delivered_pages=MAX(delivered_pages,?1) WHERE project_id=?2 AND task_id=?3",params![page+1,task.project_id.as_str(),task.task_id]).map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(
            json!({"packetHash":hash(&raw),"kind":packet["kind"],"page":page,"nextPage":if page+1<pages.len(){Some(page+1)}else{None},"totalPages":pages.len(),"passage":pages[page],"coverage":"onlySuppliedText","deliveredNotNecessarilyUnderstood":true}),
        )
    }
    pub fn submit_quality_reader(
        &mut self,
        task: &TaskRecord,
        report: &Value,
    ) -> Result<Value, RepositoryError> {
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(storage)?;
        let (raw, delivered, prior) = bound(&tx, task)?;
        let packet = parse(&raw)?;
        if report.as_object().is_none_or(|r| {
            r.len() != 5
                || [
                    "packetHash",
                    "summary",
                    "preference",
                    "findings",
                    "limitations",
                ]
                .iter()
                .any(|k| !r.contains_key(*k))
        }) || !model_record_fits(report)
        {
            return Err(invalid("Invalid reader report fields or size"));
        }
        if delivered != pages(&packet).len() || report["packetHash"] != hash(&raw) {
            return Err(invalid("Report must bind the fully delivered packet"));
        }
        if !report["summary"].as_str().is_some_and(|s| text(s, 1600))
            || !report["limitations"]
                .as_str()
                .is_some_and(|s| text(s, 1600))
            || !report["preference"]
                .as_str()
                .is_some_and(|s| valid_preference(&packet, s))
        {
            return Err(invalid(
                "Reader must state result, preference or abstention and limitations",
            ));
        }
        let findings = report["findings"]
            .as_array()
            .filter(|f| !f.is_empty() && f.len() <= 8)
            .ok_or_else(|| invalid("Reader report needs one to eight evidence findings"))?;
        for f in findings {
            if f.as_object().is_none_or(|r| {
                r.len() != 4
                    || ["label", "quote", "effect", "confidence"]
                        .iter()
                        .any(|k| !r.contains_key(*k))
            }) {
                return Err(invalid("Invalid reader finding"));
            }
            let body = packet["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["label"] == f["label"])
                .and_then(|i| i["text"].as_str())
                .ok_or_else(|| invalid("Unknown anonymous label"))?;
            if !f["quote"]
                .as_str()
                .is_some_and(|q| text(q, 1000) && body.matches(q).count() == 1)
                || !f["effect"].as_str().is_some_and(|s| text(s, 1000))
                || !matches!(f["confidence"].as_str(), Some("low" | "medium" | "high"))
            {
                return Err(invalid(
                    "Reader finding must quote actual packet text and state a reading hypothesis",
                ));
            }
        }
        if let Some(prior) = prior {
            if parse(&prior)?["report"] != *report {
                return Err(invalid(
                    "Report already saved; do not overwrite a completed review",
                ));
            }
            crate::recovery_operations::commit_operation(
                &tx,
                self.recovery_operation.as_ref(),
                &task.project_id,
                &task.task_id,
                &["submit_quality_reading"],
                &format!("runtime:quality-reading:{}", task.task_id),
            )?;
            tx.commit().map_err(storage)?;
            return parse(&prior);
        }
        let value = json!({"report":report,"taskId":task.task_id,"reviewerKind":"model","isolation":"freshTaskPacketOnly","authority":"readerHypotheses","qualityGain":"unvalidated"});
        crate::recovery_operations::commit_operation(
            &tx,
            self.recovery_operation.as_ref(),
            &task.project_id,
            &task.task_id,
            &["submit_quality_reading"],
            &format!("runtime:quality-reading:{}", task.task_id),
        )?;
        tx.execute(
            "UPDATE story_quality_packets_v1 SET report_json=?1 WHERE project_id=?2 AND task_id=?3",
            params![value.to_string(), task.project_id.as_str(), task.task_id],
        )
        .map_err(storage)?;
        tx.execute(
            "UPDATE story_quality_state_v1 SET version=version+1 WHERE project_id=?1",
            [task.project_id.as_str()],
        )
        .map_err(storage)?;
        tx.commit().map_err(storage)?;
        Ok(value)
    }
}
