use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    ObjectHead, ObjectId, RepositoryError, RevisionId, SqliteStoryRepository, TaskRecord,
    WorkingBaseline,
};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_task_source_origins_v2 (
 task_id TEXT PRIMARY KEY REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 snapshot_id TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS story_task_source_heads_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 object_id TEXT NOT NULL, object_kind TEXT NOT NULL, locked INTEGER NOT NULL,
 candidate_revision_id TEXT, accepted_revision_id TEXT, working_revision_id TEXT,
 working_selection_revision INTEGER, PRIMARY KEY(task_id,object_id)
);
CREATE TABLE IF NOT EXISTS story_task_source_knowledge_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 object_id TEXT NOT NULL, batch_id TEXT NOT NULL, PRIMARY KEY(task_id,object_id)
);
CREATE TABLE IF NOT EXISTS story_task_source_items_v2 (
 task_id TEXT NOT NULL REFERENCES story_task_source_origins_v2(task_id) ON DELETE CASCADE,
 policy_digest TEXT NOT NULL, section_path TEXT NOT NULL, source_id TEXT NOT NULL UNIQUE,
 content_hash TEXT NOT NULL, payload_json TEXT NOT NULL,
 PRIMARY KEY(task_id,policy_digest,section_path)
);
CREATE TABLE IF NOT EXISTS story_task_source_raw_returns_v1 (
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 source_id TEXT NOT NULL REFERENCES story_task_source_items_v2(source_id) ON DELETE CASCADE,
 content_hash TEXT NOT NULL, start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
 total_bytes INTEGER NOT NULL, PRIMARY KEY(task_id,source_id,start_byte,end_byte)
);
CREATE TABLE IF NOT EXISTS story_task_source_returned_v1 (
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 source_id TEXT NOT NULL REFERENCES story_task_source_items_v2(source_id) ON DELETE CASCADE,
 content_hash TEXT NOT NULL, from_entry INTEGER NOT NULL, to_entry INTEGER NOT NULL,
 total_entries INTEGER NOT NULL, field_ranges_json TEXT NOT NULL,
 PRIMARY KEY(task_id,source_id,from_entry,to_entry)
);
"#;

// References only: the revision and batch stores remain the content authorities.
pub(crate) fn capture(
    connection: &Connection,
    task: &str,
    project: &crate::ProjectId,
) -> Result<(), RepositoryError> {
    if crate::multimedia::capture_media_task(connection, task, project)? { return Ok(()); }
    connection.execute("INSERT INTO story_task_source_origins_v2 SELECT ?1,head_snapshot_id FROM story_branches_v2 WHERE project_id=?2 AND branch_id='main'", params![task,project.as_str()]).map_err(storage)?;
    connection.execute("INSERT INTO story_task_source_heads_v2 SELECT i.task_id,i.object_id,h.object_kind,h.locked,i.candidate_revision_id,i.accepted_revision_id,i.working_revision_id,i.working_selection_revision FROM story_task_input_heads_v2 i JOIN story_object_heads_v2 h ON h.project_id=?2 AND h.object_id=i.object_id WHERE i.task_id=?1", params![task,project.as_str()]).map_err(storage)?;
    connection.execute("INSERT INTO story_task_source_knowledge_v2 SELECT task_id,object_id,batch_id FROM story_task_input_knowledge_v2 WHERE task_id=?1", [task]).map_err(storage)?;
    Ok(())
}

impl SqliteStoryRepository {
    /// Current-generation findings are advisory evidence, not permission to expand scope.
    /// The source cache freezes this report under the decision task's existing capability.
    pub fn task_generation_scene_feedback(
        &self,
        task: &TaskRecord,
    ) -> Result<Option<Value>, RepositoryError> {
        let Some(bound) = crate::work_decision::binding(&self.connection, task)? else {
            return Ok(None);
        };
        let target: Value =
            serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)?;
        let Some(run) = target["runId"].as_str() else {
            return Ok(None);
        };
        self.source_task(task)?;
        let state: String = self
            .connection
            .query_row(
                "SELECT state_json FROM story_generation_runs_v1 WHERE project_id=?1 AND run_id=?2",
                params![task.project_id.as_str(), run],
                |r| r.get(0),
            )
            .map_err(storage)?;
        let state: Value =
            serde_json::from_str(&state).map_err(|_| RepositoryError::CorruptState)?;
        if state["activeTaskId"] != task.task_id {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        let Some(chapter) = state["currentChapterId"].as_str() else {
            return Ok(None);
        };
        let Some(feedback) = crate::work_decision_delivery::scene_feedback(
            &self.connection,
            &task.project_id,
            bound["workTaskId"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?,
            chapter,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(
            json!({"chapterId":chapter,"assessmentRevisionId":feedback.revision,
            "assessmentTaskId":feedback.task_id,"sourceCurrent":feedback.current,
            "unresolved":feedback.unresolved,"needsRecheck":feedback.needs_recheck(),
            "authority":"candidateAnalysis","report":feedback.content,
            "repairPolicy":"在当前作者授权内，可按实际问题提议仅修改选定场景或本章；锁定内容和未选中场景必须保留。",
            "recheckPolicy":"旧问题不会因改稿自动解决。若报告引用的正文或资料已变化，按实际依赖更新本章知识，再调用场景效果检查取得对应当前稿的报告。没有报告不代表检查通过；仍需作者决定的重要问题可如实交接。"}),
        ))
    }

    /// Initial source choices never advance with this task's own checkpoints.
    pub fn task_context_origins(
        &self,
        task: &TaskRecord,
    ) -> Result<(Vec<ObjectHead>, Vec<WorkingBaseline>), RepositoryError> {
        self.source_task(task)?;
        let mut statement = self.connection.prepare("SELECT object_id,object_kind,locked,candidate_revision_id,accepted_revision_id,working_revision_id,working_selection_revision FROM story_task_source_heads_v2 WHERE task_id=?1 ORDER BY object_id").map_err(storage)?;
        let rows = statement
            .query_map([&task.task_id], |row| {
                Ok((
                    row.get::<_, String>(/*idx*/ 0)?,
                    row.get::<_, String>(/*idx*/ 1)?,
                    row.get::<_, bool>(/*idx*/ 2)?,
                    row.get::<_, Option<String>>(/*idx*/ 3)?,
                    row.get::<_, Option<String>>(/*idx*/ 4)?,
                    row.get::<_, Option<String>>(/*idx*/ 5)?,
                    row.get::<_, Option<i64>>(/*idx*/ 6)?,
                ))
            })
            .map_err(storage)?;
        let mut heads = Vec::new();
        let mut baselines = Vec::new();
        for row in rows {
            let (object, kind, locked, candidate, accepted, working, selection) =
                row.map_err(storage)?;
            if let Some(selection_revision) = selection {
                baselines.push(WorkingBaseline {
                    object_kind: kind.clone(),
                    object_id: object.strip_prefix("draft-").unwrap_or(&kind).to_owned(),
                    revision_id: revision(working)?,
                    selection_revision,
                    selected_at: 0,
                });
            }
            heads.push(ObjectHead {
                project_id: task.project_id.clone(),
                object_id: ObjectId::parse(object).map_err(|_| RepositoryError::CorruptState)?,
                object_kind: kind,
                locked,
                candidate_revision_id: revision(candidate)?,
                accepted_revision_id: revision(accepted)?,
            });
        }
        Ok((heads, baselines))
    }

    /// Cache only a projection already admitted by the normal context visibility builder.
    /// Callers cannot register arbitrary paths or content through a public HTTP endpoint.
    pub fn project_task_sources(
        &self,
        task: &TaskRecord,
        context: &mut Value,
    ) -> Result<(), RepositoryError> {
        self.source_task(task)?;
        let policy = digest(
            &json!({"operation":task.operation,"target":serde_json::from_str::<Value>(&task.target_json).map_err(|_|RepositoryError::CorruptState)?,"position":context["target"]["position"]}),
        );
        let (known, count): (bool,i64)=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_task_source_items_v2 WHERE task_id=?1 AND policy_digest=?2),COUNT(DISTINCT policy_digest) FROM story_task_source_items_v2 WHERE task_id=?1",params![task.task_id,policy],|row|Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?))).map_err(storage)?;
        if !known && count >= 32 {
            return Err(RepositoryError::InvalidContent(
                "Task source policy cache limit reached".into(),
            ));
        }
        let mut entries = Vec::new();
        if let Some(requirements) = author_requirements(&self.connection, task)? {
            context["authorRequirementsSource"] = requirements;
        }
        // Fixed constructor-owned sections, never JSON pointers supplied by the model.
        for path in [
            "/narrativeGoals/brief",
            "/narrativeGoals/outline",
            "/plannedEffects",
            "/relevantText",
            "/relevantCommitments",
            "/candidateCommitmentProposals",
            "/sceneOutcomeRefs",
            "/operationContext/boundCommitmentInputs",
            "/operationContext/modeling",
            "/operationContext/planningReview",
            "/operationContext/modelingBatch",
            "/operationContext/entityCatalog",
            "/operationContext/workingKnowledge",
            "/operationContext/scenePlanDraftLinks",
            "/operationContext/generationSceneFeedback",
            "/operationContext/propertyRulesBase",
            "/operationContext/revisionOutcomeInput",
            "/operationContext/revisionOutcomeReport",
            "/operationContext/revisionRepairInput",
            "/operationContext/chapterStructureChange",
            "/authorRequirementsSource",
        ] {
            let Some(value) = context.pointer(path) else {
                continue;
            };
            if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
                continue;
            }
            let payload = value.to_string();
            if payload.len() > 64 * 1024 * 1024 {
                return Err(RepositoryError::InvalidContent(
                    "Visible task source exceeds its storage bound".into(),
                ));
            }
            let hash = digest(value);
            let id = format!(
                "source-{}",
                digest(&json!([
                    task.project_id.as_str(),
                    task.task_id,
                    policy,
                    path,
                    hash
                ]))
            );
            self.connection.execute("INSERT OR IGNORE INTO story_task_source_items_v2(task_id,policy_digest,section_path,source_id,content_hash,payload_json) VALUES(?1,?2,?3,?4,?5,?6)",params![task.task_id,policy,path,id,hash,payload]).map_err(storage)?;
            let (id, hash, size): (String,String,i64) = self.connection.query_row("SELECT source_id,content_hash,length(CAST(payload_json AS BLOB)) FROM story_task_source_items_v2 WHERE task_id=?1 AND policy_digest=?2 AND section_path=?3",params![task.task_id,policy,path],|row|Ok((row.get(/*idx*/0)?,row.get(/*idx*/1)?,row.get(/*idx*/2)?))).map_err(storage)?;
            let entry = json!({"sourceId":id,"sectionPath":path,"contentHash":hash,"payloadBytes":size,"chapterNumbers":source_chapter_numbers(value),"returnedCoverage":self.source_returned_coverage(task,&id,&hash)?});
            entries.push(entry.clone());
            *context
                .pointer_mut(path)
                .ok_or(RepositoryError::CorruptState)? =
                json!({"sourceId":id,"textDelivered":false});
        }
        let workspace = crate::task_workspace::register_directory(&self.connection, task)?;
        context["taskWorkspace"] = json!({"sourceId":workspace["sourceId"],"view":"workspace","textDelivered":false,"instruction":"用 read_source_content(view=workspace) 按问题、章节、对象定位，再读取所选原文；目录不证明全文阅读。"});
        entries.push(workspace);
        context["taskSources"] = json!({"authority":"frozenVisibleInput","entries":entries,"textDelivered":false,"readTool":"read_source_content","rawReadTool":"read_task_source"});
        if !entries.is_empty() {
            context["coverage"]["complete"] = json!(false);
            let missing = context["coverage"]["missingScope"]
                .as_array_mut()
                .ok_or(RepositoryError::CorruptState)?;
            if !missing.iter().any(|entry| entry == "taskSourceContents") {
                missing.push(json!("taskSourceContents"));
            }
        }
        Ok(())
    }

    // Source responses now persist transport receipts. Acquire the write snapshot
    // before reading: upgrading a deferred read while Host writes causes SQLITE_BUSY.
    pub(crate) fn with_source_return_snapshot<T>(
        &self,
        read: impl FnOnce() -> T,
    ) -> Result<T, RepositoryError> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .map_err(storage)?;
        let result = read();
        tx.commit().map_err(storage)?;
        Ok(result)
    }
    /// Read a contiguous UTF-8 page of a task-owned immutable visible projection.
    pub fn task_source_page(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.task_source_page_for_reader(task, source, cursor, &task.thread_id)
    }

    pub fn task_source_page_for_reader(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
        reader_thread_id: &str,
    ) -> Result<Value, RepositoryError> {
        self.with_source_return_snapshot(|| {
            self.source_task(task)?;
            let (path,hash,payload) = self.source_record(task, source)?;
            let fence = digest(&json!([task.project_id.as_str(),task.task_id,task.thread_id,source,hash]));
            let start = match cursor {
                None => 0,
                Some(cursor) => {
                    let (prefix,offset) = cursor.split_once(':').ok_or_else(invalid_cursor)?;
                    let offset:usize = offset.parse().map_err(|_|invalid_cursor())?;
                    if prefix!=fence || cursor!=format!("{fence}:{offset}") { return Err(invalid_cursor()); }
                    offset
                }
            };
            if start>=payload.len() || !payload.is_char_boundary(start) { return Err(invalid_cursor()); }
            let mut end = (start+2_000).min(payload.len());
            while !payload.is_char_boundary(end) {end-=1;}
            let value = json!({"sourceId":source,"sectionPath":path,"contentHash":hash,"payloadEncoding":"jsonUtf8","contentRead":false,"returnedChapterNumbers":[],"byteRange":{"start":start,"end":end,"total":payload.len()},"payloadPart":payload[start..end],"nextCursor":(end<payload.len()).then(||format!("{fence}:{end}")),"endOfInputFromCursor":end==payload.len(),"byteLimit":6500});
            if value.to_string().len()>6_500 { return Err(RepositoryError::InvalidContent("Task source page exceeds its byte budget".into())); }
            if reader_thread_id==task.thread_id {self.connection.execute("INSERT OR IGNORE INTO story_task_source_raw_returns_v1 VALUES(?1,?2,?3,?4,?5,?6)",params![task.task_id,source,hash,start,end,payload.len()]).map_err(storage)?;}
            crate::task_workspace::record_read(&self.connection,task,source,reader_thread_id)?;
            Ok(value)
        })?
    }

    /// Present bounded parsed fields, so the model selects evidence rather than repairing JSON fragments.
    pub fn task_source_content(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.task_source_content_for_reader(task, source, cursor, &task.thread_id)
    }

    pub fn task_source_content_for_reader(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
        reader_thread_id: &str,
    ) -> Result<Value, RepositoryError> {
        self.task_source_parsed(task, source, cursor, false, reader_thread_id)
    }

    /// A directory of already-visible records, with source-bound content cursors.
    /// Directory access never certifies that any record's content was read.
    pub fn task_source_index(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.task_source_parsed(task, source, cursor, true, &task.thread_id)
    }

    fn task_source_parsed(
        &self,
        task: &TaskRecord,
        source: &str,
        cursor: Option<&str>,
        index: bool,
        reader_thread_id: &str,
    ) -> Result<Value, RepositoryError> {
        self.with_source_return_snapshot(|| {
            self.source_task(task)?;
            let (section_path,hash,payload) = self.source_record(task, source)?;
            let content:Value=serde_json::from_str(&payload).map_err(|_|RepositoryError::CorruptState)?;
            let mut entries=Vec::new();
            content_entries(&content,"",&mut entries);
            let fence=digest(&json!(["parsed-fields",task.task_id,task.thread_id,source,hash]));
            if index {
                return source_index_page(task, source, &section_path, &hash, &content, &entries, &fence, cursor);
            }
            let start=match cursor {None=>0,Some(cursor)=>{
                let (prefix,index)=cursor.split_once(':').ok_or_else(invalid_cursor)?;
                let index:usize=index.parse().map_err(|_|invalid_cursor())?;
                if prefix!=fence || cursor!=format!("{fence}:{index}"){return Err(invalid_cursor())} index
            }};
            if start>=entries.len(){return Err(invalid_cursor())}
            let mut end=start;
            let mut bytes=0;
            while end<entries.len() && bytes+entries[end].to_string().len()<3400 {bytes+=entries[end].to_string().len();end+=1;}
            if end==start{return Err(RepositoryError::InvalidContent("source field metadata exceeds bounded page".into()))}
            let fields:Vec<Value>=entries[start..end].iter().map(|e|json!({"path":e["path"],"characterRange":e["characterRange"]})).collect();
            let result=crate::work_decision::bounded(json!({"sourceId":source,"sectionPath":section_path,"chapterNumbers":source_chapter_numbers(&content),"returnedChapterNumbers":returned_chapter_numbers(&content,&entries[start..end]),"contentRead":true,"contentHash":hash,"representation":"parsedFields","entries":entries[start..end],"coverage":{"fromEntry":start,"toEntry":end,"totalEntries":entries.len(),"complete":start==0 && end==entries.len()},"nextCursor":(end<entries.len()).then(||format!("{fence}:{end}"))}))?;
            if reader_thread_id==task.thread_id {self.connection.execute("INSERT OR IGNORE INTO story_task_source_returned_v1(task_id,source_id,content_hash,from_entry,to_entry,total_entries,field_ranges_json) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![task.task_id,source,hash,start,end,entries.len(),json!(fields).to_string()]).map_err(storage)?;}
            crate::task_workspace::record_read(&self.connection,task,source,reader_thread_id)?;
            Ok(result)
        })?
    }

    fn source_returned_coverage(
        &self,
        task: &TaskRecord,
        source: &str,
        hash: &str,
    ) -> Result<Value, RepositoryError> {
        let mut statement=self.connection.prepare("SELECT from_entry,to_entry,total_entries FROM story_task_source_returned_v1 WHERE task_id=?1 AND source_id=?2 AND content_hash=?3 ORDER BY from_entry,to_entry").map_err(storage)?;
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut total = 0usize;
        for row in statement
            .query_map(params![task.task_id, source, hash], |row| {
                Ok((
                    row.get::<_, usize>(0)?,
                    row.get::<_, usize>(1)?,
                    row.get::<_, usize>(2)?,
                ))
            })
            .map_err(storage)?
        {
            let (start, end, n) = row.map_err(storage)?;
            total = n;
            if let Some(last) = ranges.last_mut().filter(|r| r.1 >= start) {
                last.1 = last.1.max(end);
            } else {
                ranges.push((start, end));
            }
        }
        let next = ranges
            .first()
            .filter(|r| r.0 == 0)
            .map(|r| r.1)
            .unwrap_or(0);
        let fence = digest(&json!([
            "parsed-fields",
            task.task_id,
            task.thread_id,
            source,
            hash
        ]));
        Ok(
            json!({"authority":"returnedParsedFieldsOnly","semanticUnderstanding":false,"entryRanges":ranges.iter().take(64).map(|r|json!({"start":r.0,"end":r.1})).collect::<Vec<_>>(),"rangesComplete":ranges.len()<=64,"returnedEntries":ranges.iter().map(|r|r.1-r.0).sum::<usize>(),"totalEntries":if total==0 {Value::Null}else{json!(total)},"complete":total>0 && next==total,"nextUnreadCursor":if total>0 && next<total {json!(format!("{fence}:{next}"))}else{Value::Null}}),
        )
    }

    fn source_record(
        &self,
        task: &TaskRecord,
        source: &str,
    ) -> Result<(String, String, String), RepositoryError> {
        if let Some(record) = self.materialize_workspace_source(task, source)? {
            return Ok(record);
        }
        let record = self.connection.query_row(
            "SELECT section_path,content_hash,payload_json FROM story_task_source_items_v2 WHERE task_id=?1 AND source_id=?2",
            params![task.task_id, source], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional().map_err(storage)?;
        if let Some(record) = record {
            return Ok(record);
        }
        // Inspect only this task's admitted projections. Never disclose whether
        // a supplied ID exists under another task, or substitute a latest source.
        let has_catalog: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_task_source_items_v2 WHERE task_id=?1)",
                [&task.task_id],
                |row| row.get(0),
            )
            .map_err(storage)?;
        Err(if has_catalog {
            RepositoryError::TaskSourceNotInCatalog
        } else {
            RepositoryError::TaskSourceCatalogRequired
        })
    }

    pub(crate) fn source_task(&self, task: &TaskRecord) -> Result<(), RepositoryError> {
        if crate::tasks::load_task(&self.connection, &task.task_id)? != *task {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        crate::tasks::validate_active_task(
            &self.connection,
            task,
            &task.project_id,
            &task.thread_id,
            task.input_revision,
        )?;
        if task.capability_revoked_at.is_some() {
            return Err(RepositoryError::TaskOutputForbidden);
        }
        self.assert_task_inputs_current(task)?;
        let exists = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM story_task_source_origins_v2 WHERE task_id=?1)",
                [&task.task_id],
                |row| row.get::<_, bool>(/*idx*/ 0),
            )
            .map_err(storage)?;
        if !exists {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        Ok(())
    }
}

fn source_index_page(
    task: &TaskRecord,
    source: &str,
    section_path: &str,
    hash: &str,
    content: &Value,
    entries: &[Value],
    content_fence: &str,
    cursor: Option<&str>,
) -> Result<Value, RepositoryError> {
    let mut records = Vec::new();
    match content {
        Value::Array(items) if !items.is_empty() => {
            records.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, value)| (format!("/{i}"), value)),
            );
        }
        Value::Object(fields) if !fields.is_empty() => {
            for (key, value) in fields {
                let path = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
                if let Some(items) = value.as_array().filter(|items| !items.is_empty()) {
                    records.extend(
                        items
                            .iter()
                            .enumerate()
                            .map(|(i, value)| (format!("{path}/{i}"), value)),
                    );
                } else {
                    records.push((path, value));
                }
            }
        }
        _ => records.push((String::new(), content)),
    }
    let fence = digest(&json!([
        "source-index",
        task.task_id,
        task.thread_id,
        source,
        hash
    ]));
    let start = match cursor {
        None => 0,
        Some(cursor) => {
            let (prefix, offset) = cursor.split_once(':').ok_or_else(invalid_cursor)?;
            let offset: usize = offset.parse().map_err(|_| invalid_cursor())?;
            if prefix != fence || cursor != format!("{fence}:{offset}") {
                return Err(invalid_cursor());
            }
            offset
        }
    };
    if start >= records.len() {
        return Err(invalid_cursor());
    }
    let mut items = Vec::new();
    let mut bytes = 0;
    for (path, value) in records.iter().skip(start).take(8) {
        let belongs = |entry: &&Value| {
            entry["path"]
                .as_str()
                .is_some_and(|p| p == path || p.starts_with(&format!("{path}/")))
        };
        let first = entries
            .iter()
            .position(|entry| belongs(&entry))
            .ok_or(RepositoryError::CorruptState)?;
        let count = entries[first..].iter().take_while(belongs).count();
        let mut identity = serde_json::Map::new();
        for node in [*value, &value["content"]] {
            for key in [
                "chapterId",
                "chapterNumber",
                "revisionId",
                "sourceRevisionId",
                "title",
            ] {
                if let Some(item) = node.get(key).filter(|item| {
                    item.is_number() || item.as_str().is_some_and(|s| s.chars().count() <= 160)
                }) {
                    identity.insert(key.into(), item.clone());
                }
            }
        }
        let item = json!({"path":path,"identity":identity,"readCursor":format!("{content_fence}:{first}"),"entryRange":{"start":first,"end":first+count},"contentRead":false});
        if bytes + item.to_string().len() > 4600 {
            break;
        }
        bytes += item.to_string().len();
        items.push(item);
    }
    if items.is_empty() {
        return Err(RepositoryError::InvalidContent(
            "Source index metadata exceeds bounded page".into(),
        ));
    }
    let end = start + items.len();
    crate::work_decision::bounded(
        json!({"sourceId":source,"sectionPath":section_path,"contentHash":hash,"representation":"sourceIndex","contentRead":false,"returnedChapterNumbers":[],"items":items,"coverage":{"fromRecord":start,"toRecord":end,"totalRecords":records.len(),"contentRead":false},"nextCursor":(end<records.len()).then(||format!("{fence}:{end}")),"instruction":"Choose a record by its actual identity. Omit view and copy its readCursor to read_source_content; follow content nextCursor until entryRange.end is covered. Index metadata and a suffix do not prove complete reading of this source or other records."}),
    )
}

fn returned_chapter_numbers(content: &Value, entries: &[Value]) -> Vec<u64> {
    let mut numbers = std::collections::BTreeSet::new();
    for entry in entries {
        let mut path = entry["path"].as_str().unwrap_or("");
        loop {
            if let Some(number) = content
                .pointer(path)
                .and_then(|v| v["chapterNumber"].as_u64())
            {
                numbers.insert(number);
                break;
            }
            if path.is_empty() {
                break;
            }
            path = path
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
        }
    }
    numbers.into_iter().collect()
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn revision(value: Option<String>) -> Result<Option<RevisionId>, RepositoryError> {
    value
        .map(|value| RevisionId::parse(value).map_err(|_| RepositoryError::CorruptState))
        .transpose()
}
fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}
fn invalid_cursor() -> RepositoryError {
    RepositoryError::TaskSourceCursorInvalid
}

fn author_requirements(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Option<Value>, RepositoryError> {
    let manifest: Value = serde_json::from_str(&task.input_manifest_json)
        .map_err(|_| RepositoryError::CorruptState)?;
    if manifest["authorIntent"].as_str().is_none() {
        return Ok(None);
    }
    let revision:i64=db.query_row("SELECT COALESCE(MAX(intent_revision),0) FROM story_task_intents_v1 WHERE task_id=?1 AND status='delivered'",[&task.task_id],|r|r.get(0)).map_err(storage)?;
    Ok(Some(
        json!({"authority":"frozenAuthorIntent","intentRevision":revision,"authorText":crate::task_author_intent::source_intent(db,&task.task_id,revision)?}),
    ))
}

/// Required standards need root transport proof, irrespective of optional workspace reads.
pub(crate) fn require_author_requirements_read(
    db: &Connection,
    task: &TaskRecord,
) -> Result<(), RepositoryError> {
    let required = author_requirements(db, task)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let hash = digest(&required);
    let mut sources = db.prepare("SELECT source_id,length(CAST(payload_json AS BLOB)) FROM story_task_source_items_v2 WHERE task_id=?1 AND section_path='/authorRequirementsSource' AND content_hash=?2 ORDER BY source_id").map_err(storage)?;
    let sources = sources
        .query_map(params![task.task_id, hash], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })
        .map_err(storage)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(storage)?;
    if sources.is_empty() {
        return Err(RepositoryError::InvalidContent("先 read_context 获取 authorRequirementsSource，再以主任务完整读取作者原要求。目录/协作阅读不能代替。".into()));
    }
    let mut raw=db.prepare("SELECT start_byte,end_byte,total_bytes FROM story_task_source_raw_returns_v1 WHERE task_id=?1 AND source_id=?2 AND content_hash=?3 ORDER BY start_byte,end_byte").map_err(storage)?;
    let mut parsed=db.prepare("SELECT from_entry,to_entry,total_entries FROM story_task_source_returned_v1 WHERE task_id=?1 AND source_id=?2 AND content_hash=?3 ORDER BY from_entry,to_entry").map_err(storage)?;
    // Normal context and an assessment report can cache the same current author
    // payload under different policies. Any one complete root read suffices;
    // partial reads from different sources must never be joined.
    for (source, bytes) in sources {
        let mut through = 0usize;
        for row in raw
            .query_map(params![task.task_id, source, hash], |r| {
                Ok((
                    r.get::<_, usize>(0)?,
                    r.get::<_, usize>(1)?,
                    r.get::<_, usize>(2)?,
                ))
            })
            .map_err(storage)?
        {
            let (start, end, total) = row.map_err(storage)?;
            if total == bytes && start <= through {
                through = through.max(end)
            }
        }
        if through == bytes {
            return Ok(());
        }
        let mut through = 0usize;
        let mut total = 0usize;
        for row in parsed
            .query_map(params![task.task_id, source, hash], |r| {
                Ok((
                    r.get::<_, usize>(0)?,
                    r.get::<_, usize>(1)?,
                    r.get::<_, usize>(2)?,
                ))
            })
            .map_err(storage)?
        {
            let (start, end, n) = row.map_err(storage)?;
            total = n;
            if start <= through {
                through = through.max(end)
            }
        }
        if total > 0 && through == total {
            return Ok(());
        }
    }
    Err(RepositoryError::InvalidContent("作者原要求尚未由主任务完整读取；请沿 authorRequirementsSource.nextCursor 读取，不能用目录、后半段或协作读取代替。".into()))
}

/// Direct verification requires a complete root read of an admitted full draft.
pub(crate) fn require_workspace_revision_read(db: &Connection, task: &TaskRecord, revision: &str) -> Result<(), RepositoryError> {
    let mut q=db.prepare("SELECT i.source_id,i.content_hash,length(CAST(i.payload_json AS BLOB)) FROM story_task_source_items_v2 i JOIN story_task_workspace_refs_v1 w ON w.source_id=i.source_id AND w.task_id=i.task_id WHERE i.task_id=?1 AND w.revision_id=?2 AND w.end_scene IS NULL").map_err(storage)?;
    let rows=q.query_map(params![task.task_id,revision],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,usize>(2)?))).map_err(storage)?;
    for row in rows {
        let (source,hash,bytes)=row.map_err(storage)?;
        for (sql,expected) in [
            ("SELECT start_byte,end_byte,total_bytes FROM story_task_source_raw_returns_v1 WHERE task_id=?1 AND source_id=?2 AND content_hash=?3 ORDER BY start_byte,end_byte",Some(bytes)),
            ("SELECT from_entry,to_entry,total_entries FROM story_task_source_returned_v1 WHERE task_id=?1 AND source_id=?2 AND content_hash=?3 ORDER BY from_entry,to_entry",None),
        ] {
            let mut pages=db.prepare(sql).map_err(storage)?;
            let mut through=0usize; let mut total=0usize;
            for page in pages.query_map(params![task.task_id,source,hash],|r|Ok((r.get::<_,usize>(0)?,r.get::<_,usize>(1)?,r.get::<_,usize>(2)?))).map_err(storage)? {
                let (start,end,n)=page.map_err(storage)?;
                if expected.is_some_and(|e|e!=n) || (total!=0 && total!=n) { return Err(RepositoryError::CorruptState); }
                total=n; if start<=through {through=through.max(end);}
            }
            if total>0 && through==total {return Ok(());}
        }
    }
    Err(RepositoryError::InvalidContent("Read the current draft completely through this root task's workspace source before direct verification; directory, partial and child reads do not certify it".into()))
}

pub(crate) fn validate_author_requirement_quote(
    db: &Connection,
    project: &crate::ProjectId,
    work: &str,
    source: &str,
    content_hash: &str,
    quote: &str,
) -> Result<(), RepositoryError> {
    let row:Option<(String,String,String)>=db.query_row("SELECT i.task_id,i.content_hash,i.payload_json FROM story_task_source_items_v2 i JOIN story_tasks_v2 t ON t.task_id=i.task_id JOIN story_work_attempts_v1 a ON a.attempt_id=t.task_id JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE i.source_id=?1 AND i.section_path='/authorRequirementsSource' AND t.project_id=?2 AND s.work_task_id=?3",params![source,project.as_str(),work],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(storage)?;
    let (owner, hash, payload) = row.ok_or_else(|| RepositoryError::PayloadValidation {
        code: "author_requirement_source_not_admitted",
        instance_path: "/sourceId".into(),
        details: json!({
            "expectedSectionPath":"/authorRequirementsSource",
            "instruction":"Read this task's current authorRequirementsSource and copy its sourceId/contentHash. Quote only its authorText, which is the author's original input. revisionOutcomeInput contains model-selected criteria and engineering instructions, not original author requirements; neither it nor plans, chapterAcceptance or your own suggestions can serve as author-source evidence. Reassess the submitted requirement against the actual author input; do not guess or substitute a source."
        }),
    })?;
    if hash != content_hash {
        return Err(RepositoryError::TaskInputStale);
    }
    let task = crate::tasks::load_task(db, &owner)?;
    let required = author_requirements(db, &task)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    let latest:String=db.query_row("SELECT a.attempt_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE s.work_task_id=?1 ORDER BY a.sequence DESC LIMIT 1",[work],|r|r.get(0)).map_err(storage)?;
    let latest = crate::tasks::load_task(db, &latest)?;
    let current = author_requirements(db, &latest)?.ok_or(RepositoryError::TaskBindingInvalid)?;
    if digest(&required) != hash
        || current["authorText"] != required["authorText"]
        || serde_json::from_str::<Value>(&payload).map_err(|_| RepositoryError::CorruptState)?
            != required
    {
        return Err(RepositoryError::TaskInputStale);
    }
    crate::source_evidence::resolve(
        &json!({"quote":quote}),
        required["authorText"]
            .as_str()
            .ok_or(RepositoryError::CorruptState)?,
    )?;
    let check = crate::SourceCheck {
        source_id: source.into(),
        revision_id: String::new(),
        content_hash: hash,
        evidence: json!({"quote":quote}),
    };
    if !crate::source_check_evidence::quote_returned(db, &check)? {
        return Err(RepositoryError::InvalidContent(
            "作者原话引文须来自主任务实际读取的当前 authorRequirementsSource。".into(),
        ));
    }
    Ok(())
}

fn source_chapter_numbers(value: &Value) -> Vec<u64> {
    // Describe the frozen source, never the amount already read. If the
    // bounded walk cannot cover it, omit scope rather than publish a suffix.
    let mut pending = vec![value];
    let mut numbers = std::collections::BTreeSet::new();
    let mut visited = 0;
    while let Some(value) = pending.pop() {
        visited += 1;
        if visited > 4096 || numbers.len() > 64 {
            return Vec::new();
        }
        match value {
            Value::Object(fields) => {
                if let Some(number) = fields
                    .get("chapterNumber")
                    .and_then(Value::as_u64)
                    .filter(|number| *number > 0 && *number <= 9_007_199_254_740_991)
                {
                    numbers.insert(number);
                    if numbers.len() > 64 {
                        return Vec::new();
                    }
                }
                pending.extend(fields.values());
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    numbers.into_iter().collect()
}

#[cfg(test)]
#[path = "task_sources_author_requirements_tests.rs"]
mod author_requirements_tests;

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn source_scope_is_exact_sorted_and_never_a_truncated_suffix() {
        assert_eq!(
            source_chapter_numbers(&json!([
                {"chapterNumber":9}, {"chapterNumber":7}, {"chapterNumber":8},
                {"chapterNumber":7}, {"chapterNumber":0}, {"chapterNumber":"10"}
            ])),
            vec![7, 8, 9]
        );
        let too_many = Value::Array(
            (1..=65)
                .map(|number| json!({"chapterNumber":number}))
                .collect(),
        );
        assert!(source_chapter_numbers(&too_many).is_empty());
        let huge = Value::Array((0..5000).map(|_| json!({"chapterNumber":7})).collect());
        assert!(source_chapter_numbers(&huge).is_empty());
    }
}

fn content_entries(value: &Value, path: &str, entries: &mut Vec<Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, value) in map {
                content_entries(
                    value,
                    &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                    entries,
                )
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (i, value) in items.iter().enumerate() {
                content_entries(value, &format!("{path}/{i}"), entries)
            }
        }
        Value::String(text) if text.chars().count() > 600 => {
            let chars: Vec<char> = text.chars().collect();
            for (part, chunk) in chars.chunks(600).enumerate() {
                entries.push(json!({"path":path,"text":chunk.iter().collect::<String>(),"characterRange":{"start":part*600,"end":(part*600+chunk.len()),"total":chars.len()}}));
            }
        }
        _ => entries.push(json!({"path":path,"value":value})),
    }
}
