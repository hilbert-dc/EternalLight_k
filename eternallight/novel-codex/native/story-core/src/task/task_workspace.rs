//! A task-owned, lazy view over immutable domain revisions. Listing never reads prose.
use crate::{RepositoryError, SearchScope, SqliteStoryRepository, TaskRecord};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS story_task_workspace_refs_v1 (
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 source_id TEXT NOT NULL UNIQUE, object_id TEXT NOT NULL, revision_id TEXT NOT NULL,
 purpose TEXT NOT NULL, authority TEXT NOT NULL, working_selection_revision INTEGER,
 end_scene INTEGER, producer_task_id TEXT REFERENCES story_tasks_v2(task_id),
 output_ordinal INTEGER, CHECK((producer_task_id IS NULL)=(output_ordinal IS NULL)),
 PRIMARY KEY(task_id,source_id)
);
CREATE TABLE IF NOT EXISTS story_task_workspace_reads_v1 (
 task_id TEXT NOT NULL REFERENCES story_tasks_v2(task_id) ON DELETE CASCADE,
 source_id TEXT NOT NULL REFERENCES story_task_workspace_refs_v1(source_id) ON DELETE CASCADE,
 reader_thread_id TEXT NOT NULL, PRIMARY KEY(task_id,source_id,reader_thread_id)
);
"#;

fn storage(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(e.to_string())
}
fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
fn target(task: &TaskRecord) -> Result<Value, RepositoryError> {
    serde_json::from_str(&task.target_json).map_err(|_| RepositoryError::CorruptState)
}

pub(crate) fn descriptor(task: &TaskRecord) -> Value {
    json!({"representation":"taskWorkspace","taskId":task.task_id,"inputRevision":task.input_revision,
        "catalogueOnly":true,"contentRead":false,"readTool":"read_source_content","view":"workspace",
        "instruction":"按问题选择获准对象，再省略 view 读取其 sourceId 原文。原始输入保留冻结；savedCheckpoint 是本任务或同 Work 已实际保存的版本，可按需回读和比较。isCurrentAtListing 仅指列出时等于当前候选 head，不代表作者采纳、验收或任务完成。目录、检索命中和阅读进度不证明理解、全文阅读或没有相关事实。规划是规划，历史稿仅供比较。"})
}

pub(crate) fn directory_id(task: &TaskRecord) -> String {
    format!(
        "source-{}",
        hash(&json!([
            task.project_id.as_str(),
            task.task_id,
            "workspace",
            task.input_revision
        ]))
    )
}

pub(crate) fn register_directory(
    db: &Connection,
    task: &TaskRecord,
) -> Result<Value, RepositoryError> {
    let payload = descriptor(task);
    let id = directory_id(task);
    let digest = hash(&payload);
    db.execute("INSERT OR IGNORE INTO story_task_source_items_v2 VALUES(?1,'workspace','/taskWorkspace',?2,?3,?4)",params![task.task_id,id,digest,payload.to_string()]).map_err(storage)?;
    Ok(
        json!({"sourceId":id,"sectionPath":"/taskWorkspace","contentHash":digest,"purpose":"workspaceDirectory","payloadBytes":payload.to_string().len(),"chapterNumbers":[],"contentRead":false}),
    )
}

// Native derives prose visibility. The new directory does not open future chapters,
// hidden POV prose, or complete planning artifacts to a scene-bound writer.
struct Scope {
    from: i64,
    through: i64,
    artifacts: bool,
    end_scene: Option<usize>,
    writer: bool,
}
fn scope(repo: &SqliteStoryRepository, task: &TaskRecord) -> Result<Scope, RepositoryError> {
    let policy = crate::query_scope::TaskQueryScope::from_task(repo, task)?;
    let t = target(task)?;
    if task.operation == "assessRevisionOutcome" {
        return Ok(Scope {
            from: 1,
            through: t["chapterNumber"]
                .as_i64()
                .ok_or(RepositoryError::TaskBindingInvalid)?,
            artifacts: true,
            end_scene: None,
            writer: false,
        });
    }
    let writer = task.operation == "generateChapter";
    let (from, through, end_scene) = match policy.text_scope() {
        SearchScope::All => (1, i64::MAX, None),
        SearchScope::Chapter(n) => (n, n, None),
        SearchScope::ThroughChapter(n) => (1, n, None),
        SearchScope::ThroughScene {
            chapter,
            end_scene_order,
        } => (1, chapter, Some(end_scene_order)),
    };
    let from = if writer && t["povCharacterId"].as_str().is_some() {
        through
    } else {
        from
    };
    Ok(Scope {
        from,
        through,
        artifacts: !writer,
        end_scene,
        writer,
    })
}

#[derive(Clone)]
struct Ref {
    object: String,
    revision: String,
    purpose: String,
    authority: String,
    selection: Option<i64>,
    end_scene: Option<usize>,
    number: Option<i64>,
    title: Option<String>,
    producer: Option<String>,
    ordinal: Option<i64>,
}
impl Ref {
    fn source(&self, task: &TaskRecord) -> String {
        let mut identity = json!([
            task.project_id.as_str(),
            task.task_id,
            self.object,
            self.revision,
            self.purpose,
            self.authority,
            self.selection,
            self.end_scene
        ]);
        if self.producer.is_some() {
            identity
                .as_array_mut()
                .unwrap()
                .extend([json!(self.producer), json!(self.ordinal)]);
        }
        format!("source-{}", hash(&identity))
    }
    fn register(&self, db: &Connection, task: &TaskRecord) -> Result<Value, RepositoryError> {
        let source = self.source(task);
        db.execute(
            "INSERT OR IGNORE INTO story_task_workspace_refs_v1 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                task.task_id,
                source,
                self.object,
                self.revision,
                self.purpose,
                self.authority,
                self.selection,
                self.end_scene,
                self.producer,
                self.ordinal
            ],
        )
        .map_err(storage)?;
        let mut item = json!({"sourceId":source,"objectId":self.object,"revisionId":self.revision,"purpose":self.purpose,"authority":self.authority,"workingSelectionRevision":self.selection,"chapterNumber":self.number,"title":self.title,"contentRead":false,"contentHash":null,"instruction":"省略 view/cursor 读取本 sourceId 首段；复制原文响应中的 contentHash 引证。"});
        if let Some(producer) = &self.producer {
            let (status,completeness,current,kind):(String,Option<String>,bool,String)=db.query_row("SELECT t.status,o.completeness,EXISTS(SELECT 1 FROM story_object_heads_v2 h WHERE h.project_id=?3 AND h.object_id=?4 AND h.candidate_revision_id=?5),o.output_kind FROM story_task_outputs_v2 o JOIN story_tasks_v2 t ON t.task_id=o.task_id WHERE o.task_id=?1 AND o.ordinal=?2",params![producer,self.ordinal,task.project_id.as_str(),self.object,self.revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(storage)?;
            item["producerTaskId"] = json!(producer);
            item["outputKind"] = json!(kind);
            item["outputOrdinal"] = json!(self.ordinal);
            item["version"] = json!({"revisionId":self.revision,"outputOrdinal":self.ordinal});
            item["producerStatusAtListing"] = json!(status);
            item["completeness"] = json!(completeness);
            item["isCurrentAtListing"] = json!(current);
            item["currentMeaning"] = json!("equalsCandidateHeadAtListing");
            item["evidenceUse"] =
                json!("immutableSavedVersionForReadingAndComparison;notAcceptanceOrDelivery");
        }
        Ok(item)
    }
}

/// Resolve a Native-derived verification action, never a model-supplied locator.
/// Registration is metadata only; normal source reads still enforce full coverage.
pub(crate) fn verification_source(
    db: &Connection, task: &TaskRecord, writer: &str, revision: &str,
) -> Result<Value, RepositoryError> {
    if task.operation != "advisory" {
        return Err(RepositoryError::TaskBindingInvalid);
    }
    let (object, number, ordinal): (String, i64, i64) = db.query_row(
        "SELECT r.object_id,c.number,o.ordinal FROM story_task_outputs_v2 o JOIN story_revisions_v2 r ON r.revision_id=o.revision_id JOIN story_chapters_v2 c ON r.object_id='draft-'||c.chapter_id AND c.project_id=r.project_id JOIN story_object_heads_v2 h ON h.object_id=r.object_id AND h.project_id=r.project_id AND h.candidate_revision_id=r.revision_id WHERE o.task_id=?1 AND o.revision_id=?2 AND o.output_kind='draft' AND r.project_id=?3",
        params![writer, revision, task.project_id.as_str()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(storage)?.ok_or(RepositoryError::TaskSourceNotInCatalog)?;
    let reference = Ref { object, revision: revision.into(), purpose: "savedCheckpoint".into(),
        authority: "nativeSavedCheckpoint".into(), selection: None, end_scene: None,
        number: Some(number), title: None, producer: Some(writer.into()), ordinal: Some(ordinal) };
    if !ref_current(db, task, &reference, ReferenceValidation::ActiveRead)? {
        return Err(RepositoryError::TaskSourceNotInCatalog);
    }
    let item = reference.register(db, task)?;
    Ok(json!({"revisionId":revision,"contentRead":false,"read":{"tool":"read_source_content",
        "arguments":{"sourceId":item["sourceId"],"expectedRevision":task.input_revision}}}))
}

impl SqliteStoryRepository {
    

    

    

    

    fn workspace_page(&self, task: &TaskRecord, source: Option<&str>, cursor: Option<&str>,
        chapter_number: Option<i64>, object_kind: Option<&str>, query: Option<&str>,
        revision_id: Option<&str>, excerpt: Option<(&str, usize)>, reader: &str) -> Result<Value, RepositoryError> {
        self.with_source_return_snapshot(||{
            self.source_task(task)?;
            let current_directory = directory_id(task);
            if source.is_none() { register_directory(&self.connection, task)?; }
            let source = source.unwrap_or(&current_directory);
            if revision_id.is_some_and(|id| id.is_empty() || id.len() > 160) { return Err(RepositoryError::TaskBindingInvalid); }
            if source!=directory_id(task) || !self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_task_source_items_v2 WHERE task_id=?1 AND source_id=?2)",params![task.task_id,source],|r|r.get::<_,bool>(0)).map_err(storage)? {return Err(RepositoryError::TaskSourceNotInCatalog)}
            if chapter_number.is_some_and(|n|n<=0) || object_kind.is_some_and(|k|!matches!(k,"draft"|"brief"|"outline"|"plan"|"propertyRules"|"revisionOutcome"|"sceneAssessment"|"commitmentExtraction"|"critique"|"commitment")) || query.is_some_and(|q|q.trim().is_empty()||q.chars().count()>120) {return Err(RepositoryError::TaskBindingInvalid)}
            let s=scope(self,task)?; let t=target(task)?;
            let root_valid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM story_work_attempts_v1 a JOIN story_work_steps_v1 rs ON rs.id=a.step_id AND rs.current_attempt_id=a.attempt_id JOIN story_work_tasks_v1 w ON w.id=rs.work_task_id WHERE a.attempt_id=?1 AND w.project_id=?2 AND w.status='running' AND w.thread_id!='' AND w.thread_id=(SELECT t.thread_id FROM story_work_attempts_v1 first JOIN story_work_steps_v1 fs ON fs.id=first.step_id JOIN story_tasks_v2 t ON t.task_id=first.attempt_id AND t.project_id=w.project_id WHERE fs.work_task_id=w.id ORDER BY first.sequence LIMIT 1))",params![task.task_id,task.project_id.as_str()],|r|r.get(0)).map_err(storage)?;
            if !root_valid{return Err(RepositoryError::TaskBindingInvalid)}
            let catalogue_revision:(i64,i64,i64)=self.connection.query_row("SELECT COUNT(o.rowid),COALESCE(MAX(o.rowid),0),w.version FROM story_work_attempts_v1 reader JOIN story_work_steps_v1 rs ON rs.id=reader.step_id JOIN story_work_tasks_v1 w ON w.id=rs.work_task_id LEFT JOIN story_work_steps_v1 ps ON ps.work_task_id=w.id LEFT JOIN story_work_attempts_v1 producer ON producer.step_id=ps.id AND producer.sequence<=reader.sequence LEFT JOIN story_task_outputs_v2 o ON o.task_id=producer.attempt_id WHERE reader.attempt_id=?1 AND w.project_id=?2",params![task.task_id,task.project_id.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(storage)?;
            let fence=hash(&json!([source,"workspace",chapter_number,object_kind,query,revision_id,catalogue_revision,excerpt]));
            let start=match cursor{None=>0,Some(c)=>{let(p,o)=c.split_once(':').ok_or(RepositoryError::TaskSourceCursorInvalid)?;let o:usize=o.parse().map_err(|_|RepositoryError::TaskSourceCursorInvalid)?;if p!=fence||c!=format!("{fence}:{o}"){return Err(RepositoryError::TaskSourceCursorInvalid)}o}};
            let candidate=crate::query_scope::uses_candidate_source(task)?;
            // Select a bounded block of metadata. SQLite never returns full bodies here.
            // Writer search uses metadata only so hidden suffixes cannot leak as matches.
            let mut stmt=self.connection.prepare(r#"
WITH reader AS (
 SELECT a.sequence,w.id,w.thread_id FROM story_work_attempts_v1 a
 JOIN story_work_steps_v1 s ON s.id=a.step_id AND s.current_attempt_id=a.attempt_id
 JOIN story_work_tasks_v1 w ON w.id=s.work_task_id
 WHERE a.attempt_id=?1 AND w.project_id=?2 AND w.status='running' AND w.thread_id!=''
), catalogue AS (
 SELECT h.object_id,h.object_kind,
 CASE WHEN ?3 THEN COALESCE(h.candidate_revision_id,h.accepted_revision_id)
 ELSE COALESCE(h.working_revision_id,h.accepted_revision_id,h.candidate_revision_id) END AS revision_id,
 h.working_selection_revision,h.working_revision_id,h.accepted_revision_id,
 NULL AS producer_task_id,NULL AS output_ordinal,0 AS producer_sequence,NULL AS chapter_id
 FROM story_task_source_heads_v2 h WHERE h.task_id=?1 AND h.object_kind IN ('draft','brief','outline','plan','propertyRules')
 UNION ALL
 SELECT r.object_id,o.output_kind,o.revision_id,NULL,NULL,NULL,p.task_id,o.ordinal,a.sequence,json_extract(p.target_json,'$.chapterId')
 FROM story_task_outputs_v2 o JOIN story_tasks_v2 p ON p.task_id=o.task_id
 JOIN story_revisions_v2 r ON r.revision_id=o.revision_id AND r.project_id=p.project_id
 JOIN story_work_attempts_v1 a ON a.attempt_id=p.task_id
 JOIN story_work_steps_v1 ps ON ps.id=a.step_id AND ps.operation=p.operation
 JOIN reader ON reader.id=ps.work_task_id AND a.sequence<=reader.sequence
 WHERE p.project_id=?2 AND o.output_kind=r.object_kind AND o.output_kind=p.required_output_kind
 AND json_extract(p.input_manifest_json,'$.outputObjectId')=r.object_id
 AND (o.output_kind!='draft' OR r.object_id='draft-'||json_extract(p.target_json,'$.chapterId'))
 AND EXISTS(SELECT 1 FROM json_each(p.output_refs_json) j
   WHERE json_extract(j.value,'$.revisionId')=o.revision_id
   AND json_extract(j.value,'$.objectId')=r.object_id AND json_extract(j.value,'$.kind')=o.output_kind)
 UNION ALL
 SELECT 'planning-review-'||run.run_id,'critique',report.report_id,NULL,NULL,NULL,p.task_id,o.ordinal,a.sequence,NULL
 FROM story_task_outputs_v2 o JOIN story_tasks_v2 p ON p.task_id=o.task_id
 JOIN story_planning_review_reports_v2 report ON report.task_id=p.task_id AND report.report_id=o.revision_id
 JOIN story_planning_review_runs_v2 run ON run.run_id=report.run_id AND run.project_id=p.project_id
 JOIN story_planning_review_seats_v2 seat ON seat.run_id=run.run_id AND seat.task_id=p.task_id AND seat.role=report.role
 JOIN story_work_attempts_v1 a ON a.attempt_id=p.task_id
 JOIN story_work_steps_v1 ps ON ps.id=a.step_id AND ps.operation=p.operation AND ps.work_task_id=run.work_task_id
 JOIN reader ON reader.id=ps.work_task_id AND a.sequence<=reader.sequence
 WHERE p.project_id=?2 AND p.operation='reviewPlanningSeat' AND p.required_output_kind='critique' AND o.output_kind='critique'
 AND json_extract(p.target_json,'$.reviewRunId')=run.run_id AND json_extract(p.target_json,'$.role')=report.role
 AND EXISTS(SELECT 1 FROM json_each(p.output_refs_json) j
 WHERE json_extract(j.value,'$.revisionId')=report.report_id
 AND json_extract(j.value,'$.objectId')='planning-review-'||run.run_id AND json_extract(j.value,'$.kind')='critique')
)
SELECT h.object_id,h.object_kind,h.revision_id,h.working_selection_revision,h.working_revision_id,
 h.accepted_revision_id,c.number,CASE WHEN json_type(r.content_json,'$.title')='text' THEN substr(json_extract(r.content_json,'$.title'),1,120) END,
 CASE WHEN ?10 IS NULL THEN 1
 WHEN instr(COALESCE(CASE WHEN json_type(r.content_json,'$.title')='text' THEN substr(json_extract(r.content_json,'$.title'),1,120) END,'')||h.object_id,?10)>0 THEN 1
 WHEN NOT ?11 THEN instr(COALESCE(json_extract(r.content_json,'$.body'),r.content_json,''),?10)>0 ELSE 0 END,
 h.producer_task_id,h.output_ordinal
FROM catalogue h LEFT JOIN story_revisions_v2 r ON r.project_id=?2 AND r.object_id=h.object_id AND r.revision_id=h.revision_id
LEFT JOIN story_chapters_v2 c ON c.project_id=?2 AND (h.object_id='draft-'||c.chapter_id OR (h.object_kind IN ('revisionOutcome','sceneAssessment','commitmentExtraction') AND h.chapter_id=c.chapter_id))
WHERE ((h.object_kind='draft' AND c.number BETWEEN ?4 AND ?5)
 OR (?6 AND h.object_kind IN ('brief','outline','plan','propertyRules','critique','commitment'))
 OR (?6 AND h.object_kind IN ('revisionOutcome','sceneAssessment','commitmentExtraction') AND c.number BETWEEN ?4 AND ?5))
AND (r.revision_id IS NOT NULL OR h.object_kind='critique')
AND (?7 IS NULL OR c.number=?7) AND (?8 IS NULL OR h.object_kind=?8) AND (?12 IS NULL OR h.revision_id=?12)
AND (?13=0 OR (h.object_kind='draft' AND h.producer_task_id IS NULL))
ORDER BY COALESCE(c.number,0),h.object_id,h.producer_task_id IS NOT NULL,h.producer_sequence,h.output_ordinal
LIMIT 65 OFFSET ?9
"#).map_err(storage)?;
            let rows=stmt.query_map(params![task.task_id,task.project_id.as_str(),candidate,s.from,s.through,s.artifacts,chapter_number,object_kind,start,query,s.writer,revision_id,excerpt.is_some()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<i64>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,bool>(8)?,r.get::<_,Option<String>>(9)?,r.get::<_,Option<i64>>(10)?))).map_err(storage)?;
            let include_before=excerpt.is_none() && revision_id.is_none_or(|r|t["originalRevisionId"].as_str()==Some(r)) && start==0 && task.operation=="assessRevisionOutcome" && t["originalRevisionId"].as_str().is_some() && query.is_none() && object_kind.is_none_or(|k|k=="draft") && chapter_number.is_none_or(|n|Some(n)==t["chapterNumber"].as_i64());
            let page_limit=if include_before{7}else{8};
            let mut items=Vec::new();let mut scanned=0;let mut more=false;let mut item_bytes=0;
            for row in rows {
                let(object,kind,revision,selection,working,accepted,number,title,matches,producer,ordinal)=row.map_err(storage)?;
                if scanned==64||items.len()==page_limit{more=true;break} scanned+=1;
                if !matches{continue}
                let is_working=!candidate&&selection.is_some()&&working.as_deref()==Some(&revision);
                let writer_input=s.writer&&number==t["chapterNumber"].as_i64();
                let own_input=producer.is_none() && serde_json::from_str::<Value>(&task.input_manifest_json).map_err(|_|RepositoryError::CorruptState)?["outputObjectId"]==object && matches!(kind.as_str(),"brief"|"outline"|"plan"|"propertyRules");
                let authority=if producer.is_some()&&kind=="critique"{"nativePlanningReport"}else if producer.is_some(){"nativeSavedCheckpoint"}else if writer_input{"frozenWriterInput"}else if own_input{"frozenTaskInput"}else if is_working{"working"}else if accepted.as_deref()==Some(&revision){"accepted"}else{"candidate"};
                let purpose=if producer.is_some()&&matches!(kind.as_str(),"revisionOutcome"|"sceneAssessment"|"commitmentExtraction"|"critique"|"commitment"){"savedReport"}else if producer.is_some(){"savedCheckpoint"}else if kind!="draft"{"planning"}else if writer_input{"writerInput"}else if number==t["chapterNumber"].as_i64(){"currentDraft"}else{"priorText"};
                let mut item=Ref{object,revision,purpose:purpose.into(),authority:authority.into(),selection:if is_working{selection}else{None},end_scene:if number==Some(s.through){s.end_scene}else{None},number,title:title.map(|s|s.chars().take(120).collect()),producer,ordinal}.register(&self.connection,task)?;
                if let Some((edge, characters)) = excerpt {
                    let id = item["sourceId"].as_str().ok_or(RepositoryError::CorruptState)?;
                    let (_, digest, payload) = self.materialize_workspace_source(task, id)?.ok_or(RepositoryError::TaskSourceNotInCatalog)?;
                    let content: Value = serde_json::from_str(&payload).map_err(|_|RepositoryError::CorruptState)?;
                    let passage = crate::source_edit_anchors::boundary_excerpt(&content, edge, characters);
                    item.as_object_mut().unwrap().retain(|key, _| ["sourceId","revisionId","chapterNumber","purpose","authority"].contains(&key.as_str()));
                    item["contentHash"] = json!(digest);
                    item["contentRead"] = json!(passage.is_some());
                    item["excerpt"] = json!(passage);
                    if item["contentRead"] == true {
                        item["sceneRead"] = json!({"tool":"read_source_content","arguments":{
                            "expectedRevision":task.input_revision,"sourceId":item["sourceId"],"view":"scene",
                            "groupId":item["excerpt"]["editAnchor"]["groupId"]}});
                    }
                }
                if item_bytes+item.to_string().len()>4300 {
                    if items.is_empty() { return Err(RepositoryError::InvalidContent("Serialized excerpt exceeds page capacity; reduce characters or read the source directly".into())); }
                    scanned-=1;more=true;break
                }
                if excerpt.is_some() && item["contentRead"] == true {
                    record_read(&self.connection, task, item["sourceId"].as_str().ok_or(RepositoryError::CorruptState)?, reader)?;
                }
                item_bytes+=item.to_string().len();items.push(item);
            }
            if include_before {
                if let Some(revision)=t["originalRevisionId"].as_str(){
                    let original=Ref{object:format!("draft-{}",t["chapterId"].as_str().ok_or(RepositoryError::TaskBindingInvalid)?),revision:revision.into(),purpose:"beforeDraft".into(),authority:"frozenComparison".into(),selection:None,end_scene:None,number:t["chapterNumber"].as_i64(),title:None,producer:None,ordinal:None}.register(&self.connection,task)?;
                    items.push(original);
                }
            }
            if scanned==0&&start>0{return Err(RepositoryError::TaskSourceCursorInvalid)}
            if let Some((edge, characters)) = excerpt {
                return crate::work_decision::bounded(json!({"sourceId":source,"representation":"workspacePassages",
                    "selection":"frozenTaskBaseline","edge":edge,"characters":characters,"boundary":"visibleProseGroup",
                    "contentRead":items.iter().any(|i|i["contentRead"]==true),"items":items,
                    "visibility":{"prose":{"fromChapter":s.from,"throughChapter":if s.through==i64::MAX{None}else{Some(s.through)},"throughScene":s.end_scene},"existenceOutsideScope":"unknown"},
                    "coverage":{"kind":"partialProse","completeSource":false,"scannedRecords":scanned,"searchScopeCompleteFromCursor":!more},
                    "nextCursor":more.then(||format!("{fence}:{}",start+scanned)),
                    "instruction":"Exact first/last characters of the first/last non-empty visible prose group in each frozen chapter source. Partial original text, not a summary or complete reading. A clipped visible boundary may not be the chapter boundary. Use sourceId for more context; missing or truncated text does not prove absence or acceptance."}));
            }
            crate::work_decision::bounded(json!({"sourceId":source,"representation":"workspaceDirectory","contentRead":false,"items":items,"visibility":{"planningArtifacts":if s.artifacts{"eligibleWithinTaskScope"}else{"excludedByWriterScope"},"prose":{"fromChapter":s.from,"throughChapter":if s.through==i64::MAX{None}else{Some(s.through)},"throughScene":s.end_scene},"existenceOutsideScope":"unknown"},"coverage":{"scannedRecords":scanned,"contentRead":false,"semanticCompleteness":"unknown","searchScopeCompleteFromCursor":!more,"queryMode":if s.writer{"visibleMetadataOnly"}else{"literalVisibleSourceLocator"}},"nextCursor":more.then(||format!("{fence}:{}",start+scanned)),"instruction":"按来源用途读取原文并做判断。savedCheckpoint 是实际保存的 immutable 版本，isCurrentAtListing 只观察列出时的候选 head；历史版本不能证明最新稿验收。每页只定位一个有界记录块；未命中或最后一页不是语义上的不存在证明。规划/假设不能当作已发生的正文事实。"}))
        })?
    }

    pub(crate) fn materialize_workspace_source(
        &self,
        task: &TaskRecord,
        source: &str,
    ) -> Result<Option<(String, String, String)>, RepositoryError> {
        let Some(r) = load_ref(&self.connection, &task.task_id, source)? else {
            return Ok(None);
        };
        if r.source(task) != source {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        if !ref_current(&self.connection, task, &r, ReferenceValidation::ActiveRead)? {
            return Err(RepositoryError::TaskInputStale);
        }
        let cached=self.connection.query_row("SELECT section_path,content_hash,payload_json FROM story_task_source_items_v2 WHERE task_id=?1 AND source_id=?2",params![task.task_id,source],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(storage)?;
        if cached.is_some() {
            return Ok(cached);
        }
        let raw: Option<String> = if r.authority == "nativePlanningReport" {
            self.connection.query_row("SELECT CASE WHEN length(CAST(report.report_json AS BLOB))<=8388608 THEN report.report_json END FROM story_planning_review_reports_v2 report JOIN story_planning_review_runs_v2 run ON run.run_id=report.run_id WHERE run.project_id=?1 AND 'planning-review-'||run.run_id=?2 AND report.report_id=?3 AND report.task_id=?4",params![task.project_id.as_str(),r.object,r.revision,r.producer],|row|row.get(0)).optional().map_err(storage)?.flatten()
        } else {
            self.connection.query_row("SELECT CASE WHEN length(CAST(content_json AS BLOB))<=8388608 THEN content_json END FROM story_revisions_v2 WHERE project_id=?1 AND object_id=?2 AND revision_id=?3",params![task.project_id.as_str(),r.object,r.revision],|row|row.get(0)).optional().map_err(storage)?.flatten()
        };
        let mut content: Value = serde_json::from_str(&raw.ok_or_else(|| {
            RepositoryError::InvalidContent("Workspace object exceeds bounded source size".into())
        })?)
        .map_err(|_| RepositoryError::CorruptState)?;
        if let Some(end) = r.end_scene {
            let groups = content["sceneGroups"]
                .as_array_mut()
                .ok_or(RepositoryError::CorruptState)?;
            groups.truncate(end);
            let bodies = groups
                .iter()
                .map(|g| {
                    g["body"]
                        .as_str()
                        .map(str::trim)
                        .ok_or(RepositoryError::CorruptState)
                })
                .collect::<Result<Vec<_>, _>>()?;
            content["body"] = json!(bodies.join("\n\n---\n\n"));
        }
        let mut payload = json!({"objectId":r.object,"sourceRevisionId":r.revision,"purpose":r.purpose,"authority":r.authority,"workingSelectionRevision":r.selection,"content":content});
        if r.object.starts_with("draft-") {
            crate::project_draft_source_text(&mut payload);
        }
        if r.producer.is_some() {
            payload["producerTaskId"] = json!(r.producer);
            payload["outputOrdinal"] = json!(r.ordinal);
            payload["evidenceUse"] =
                json!("immutableSavedVersionForReadingAndComparison;notAcceptanceOrDelivery");
        }
        let mut path = format!("/workspace/{}/{}", r.object, r.purpose);
        if let Some(producer) = &r.producer {
            path.push_str(&format!(
                "/{producer}/{}",
                r.ordinal.ok_or(RepositoryError::CorruptState)?
            ));
        }
        let digest = hash(&payload);
        let raw = payload.to_string();
        self.connection
            .execute(
                "INSERT INTO story_task_source_items_v2 VALUES(?1,'workspaceObject',?2,?3,?4,?5)",
                params![task.task_id, path, source, digest, raw],
            )
            .map_err(storage)?;
        Ok(Some((path, digest, raw)))
    }
}

fn load_ref(db: &Connection, task: &str, source: &str) -> Result<Option<Ref>, RepositoryError> {
    db.query_row("SELECT object_id,revision_id,purpose,authority,working_selection_revision,end_scene,producer_task_id,output_ordinal FROM story_task_workspace_refs_v1 WHERE task_id=?1 AND source_id=?2",params![task,source],|r|Ok(Ref{object:r.get(0)?,revision:r.get(1)?,purpose:r.get(2)?,authority:r.get(3)?,selection:r.get(4)?,end_scene:r.get(5)?,producer:r.get(6)?,ordinal:r.get(7)?,number:None,title:None})).optional().map_err(storage)
}
#[derive(Clone, Copy)]
enum ReferenceValidation {
    ActiveRead,
    RecordedDependency,
}

fn ref_current(
    db: &Connection,
    task: &TaskRecord,
    r: &Ref,
    validation: ReferenceValidation,
) -> Result<bool, RepositoryError> {
    // A recorded immutable source remains a fact after its Work stops. This
    // private mode grants neither source materialization nor execution rights.
    let recorded_dependency = matches!(validation, ReferenceValidation::RecordedDependency);
    if r.authority == "nativePlanningReport" {
        if r.purpose != "savedReport" || r.producer.is_none() || r.ordinal.is_none_or(|n| n <= 0) {
            return Ok(false);
        }
        return db.query_row(r#"
SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o JOIN story_tasks_v2 p ON p.task_id=o.task_id
JOIN story_planning_review_reports_v2 report ON report.task_id=p.task_id AND report.report_id=o.revision_id
JOIN story_planning_review_runs_v2 run ON run.run_id=report.run_id AND run.project_id=p.project_id
JOIN story_planning_review_seats_v2 seat ON seat.run_id=run.run_id AND seat.task_id=p.task_id AND seat.role=report.role
JOIN story_work_attempts_v1 producer ON producer.attempt_id=p.task_id
JOIN story_work_steps_v1 ps ON ps.id=producer.step_id AND ps.operation=p.operation AND ps.work_task_id=run.work_task_id
JOIN story_work_tasks_v1 w ON w.id=ps.work_task_id
JOIN story_work_attempts_v1 reader ON reader.attempt_id=?1 AND reader.sequence>=producer.sequence
JOIN story_work_steps_v1 rs ON rs.id=reader.step_id AND rs.work_task_id=w.id AND rs.current_attempt_id=reader.attempt_id
WHERE w.project_id=?2 AND p.project_id=?2 AND w.thread_id!='' AND (?7 OR w.status='running')
AND w.thread_id=(SELECT root.thread_id FROM story_work_attempts_v1 first JOIN story_work_steps_v1 fs ON fs.id=first.step_id JOIN story_tasks_v2 root ON root.task_id=first.attempt_id AND root.project_id=w.project_id WHERE fs.work_task_id=w.id ORDER BY first.sequence LIMIT 1)
AND o.task_id=?3 AND o.ordinal=?4 AND report.report_id=?5 AND 'planning-review-'||run.run_id=?6
AND p.operation='reviewPlanningSeat' AND p.required_output_kind='critique' AND o.output_kind='critique'
AND json_extract(p.target_json,'$.reviewRunId')=run.run_id AND json_extract(p.target_json,'$.role')=report.role
AND EXISTS(SELECT 1 FROM json_each(p.output_refs_json) j WHERE json_extract(j.value,'$.revisionId')=report.report_id
AND json_extract(j.value,'$.objectId')='planning-review-'||run.run_id AND json_extract(j.value,'$.kind')='critique'))
"#,params![task.task_id,task.project_id.as_str(),r.producer,r.ordinal,r.revision,r.object,recorded_dependency],|row|row.get(0)).map_err(storage);
    }
    if r.authority == "nativeSavedCheckpoint" {
        // An immutable saved version remains usable as comparison evidence after
        // another legal save. This never substitutes for current-target acceptance.
        if !matches!(r.purpose.as_str(), "savedCheckpoint" | "savedReport")
            || r.producer.is_none()
            || r.ordinal.is_none_or(|n| n <= 0)
        {
            return Ok(false);
        }
        return db.query_row(r#"
SELECT EXISTS(SELECT 1 FROM story_task_outputs_v2 o
JOIN story_tasks_v2 p ON p.task_id=o.task_id
JOIN story_revisions_v2 revision ON revision.revision_id=o.revision_id AND revision.project_id=p.project_id
JOIN story_work_attempts_v1 producer ON producer.attempt_id=p.task_id
JOIN story_work_steps_v1 ps ON ps.id=producer.step_id AND ps.operation=p.operation
JOIN story_work_tasks_v1 w ON w.id=ps.work_task_id
JOIN story_work_attempts_v1 reader ON reader.attempt_id=?1 AND reader.sequence>=producer.sequence
JOIN story_work_steps_v1 rs ON rs.id=reader.step_id AND rs.work_task_id=w.id AND rs.current_attempt_id=reader.attempt_id
WHERE w.project_id=?2 AND p.project_id=?2 AND w.thread_id!='' AND (?7 OR w.status='running')
AND w.thread_id=(SELECT root.thread_id FROM story_work_attempts_v1 first JOIN story_work_steps_v1 fs ON fs.id=first.step_id JOIN story_tasks_v2 root ON root.task_id=first.attempt_id AND root.project_id=w.project_id WHERE fs.work_task_id=w.id ORDER BY first.sequence LIMIT 1)
AND o.task_id=?3 AND o.ordinal=?4 AND o.revision_id=?5 AND revision.object_id=?6
AND o.output_kind=revision.object_kind AND o.output_kind=p.required_output_kind
AND o.output_kind IN ('draft','brief','outline','plan','propertyRules','revisionOutcome','sceneAssessment','commitmentExtraction','commitment')
AND json_extract(p.input_manifest_json,'$.outputObjectId')=revision.object_id
AND (o.output_kind!='draft' OR revision.object_id='draft-'||json_extract(p.target_json,'$.chapterId'))
AND EXISTS(SELECT 1 FROM json_each(p.output_refs_json) j
WHERE json_extract(j.value,'$.revisionId')=o.revision_id
AND json_extract(j.value,'$.objectId')=revision.object_id AND json_extract(j.value,'$.kind')=o.output_kind))
"#,params![task.task_id,task.project_id.as_str(),r.producer,r.ordinal,r.revision,r.object,recorded_dependency],|row|row.get(0)).map_err(storage);
    }
    if r.authority == "frozenComparison" {
        let t = target(task)?;
        return Ok(task.operation == "assessRevisionOutcome"
            && t["originalRevisionId"] == r.revision
            && r.object
                == format!(
                    "draft-{}",
                    t["chapterId"]
                        .as_str()
                        .ok_or(RepositoryError::TaskBindingInvalid)?
                ));
    }
    if matches!(
        r.authority.as_str(),
        "frozenWriterInput" | "frozenTaskInput"
    ) {
        let t = target(task)?;
        let writer = r.authority == "frozenWriterInput";
        if (writer
            && (task.operation != "generateChapter"
                || r.object
                    != format!(
                        "draft-{}",
                        t["chapterId"]
                            .as_str()
                            .ok_or(RepositoryError::TaskBindingInvalid)?
                    )))
            || (!writer
                && (serde_json::from_str::<Value>(&task.input_manifest_json)
                    .map_err(|_| RepositoryError::CorruptState)?["outputObjectId"]
                    != r.object
                    || !matches!(
                        task.operation.as_str(),
                        "generateBrief"
                            | "generateOutline"
                            | "generatePlan"
                            | "generatePropertyRules"
                    )))
        {
            return Ok(false);
        }
        let candidate = crate::query_scope::uses_candidate_source(task)?;
        return db.query_row("SELECT EXISTS(SELECT 1 FROM story_task_source_heads_v2 h WHERE h.task_id=?1 AND h.object_id=?2 AND CASE WHEN ?3 THEN COALESCE(h.candidate_revision_id,h.accepted_revision_id) ELSE COALESCE(h.working_revision_id,h.accepted_revision_id,h.candidate_revision_id) END=?4)",params![task.task_id,r.object,candidate,r.revision],|row|row.get(0)).map_err(storage);
    }
    let current:Option<(Option<String>,Option<String>,Option<String>,Option<i64>)>=db.query_row("SELECT h.candidate_revision_id,h.accepted_revision_id,b.revision_id,b.selection_revision FROM story_object_heads_v2 h LEFT JOIN story_working_baselines_v2 b ON b.project_id=h.project_id AND b.object_kind=h.object_kind AND b.object_id=CASE WHEN h.object_kind='draft' THEN substr(h.object_id,7) ELSE h.object_kind END WHERE h.project_id=?1 AND h.object_id=?2",params![task.project_id.as_str(),r.object],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(storage)?;
    let Some((candidate, accepted, working, selection)) = current else {
        return Ok(false);
    };
    Ok(match r.authority.as_str() {
        "working" => r.selection == selection && working.as_deref() == Some(&r.revision),
        "accepted" => accepted.as_deref() == Some(&r.revision),
        "candidate" => candidate.as_deref().or(accepted.as_deref()) == Some(&r.revision),
        _ => false,
    })
}

pub(crate) fn record_read(
    db: &Connection,
    task: &TaskRecord,
    source: &str,
    reader: &str,
) -> Result<(), RepositoryError> {
    db.execute("INSERT OR IGNORE INTO story_task_workspace_reads_v1 SELECT task_id,source_id,?3 FROM story_task_workspace_refs_v1 WHERE task_id=?1 AND source_id=?2",params![task.task_id,source,reader]).map_err(storage)?;
    Ok(())
}
/// Empty optional dependencies do not certify required draft/author requirements.
/// Callers must additionally validate the mandatory domain target and author intent.
pub(crate) fn dependencies_current(
    db: &Connection,
    task_id: &str,
) -> Result<bool, RepositoryError> {
    let task = crate::tasks::load_task(db, task_id)?;
    let mut q = db
        .prepare("SELECT DISTINCT source_id FROM story_task_workspace_reads_v1 WHERE task_id=?1")
        .map_err(storage)?;
    for id in q
        .query_map([task_id], |r| r.get::<_, String>(0))
        .map_err(storage)?
    {
        let id = id.map_err(storage)?;
        let r = load_ref(db, task_id, &id)?.ok_or(RepositoryError::CorruptState)?;
        if r.source(&task) != id
            || !ref_current(db, &task, &r, ReferenceValidation::RecordedDependency)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn frozen_comparison(
    db: &Connection,
    source: &str,
    project: &str,
    revision: &str,
) -> Result<bool, RepositoryError> {
    let task_id:Option<String>=db.query_row("SELECT w.task_id FROM story_task_workspace_refs_v1 w JOIN story_tasks_v2 t ON t.task_id=w.task_id WHERE w.source_id=?1 AND w.revision_id=?2 AND w.authority='frozenComparison' AND t.project_id=?3",params![source,revision,project],|r|r.get(0)).optional().map_err(storage)?;
    let Some(id) = task_id else { return Ok(false) };
    let task = crate::tasks::load_task(db, &id)?;
    let r = load_ref(db, &id, source)?.ok_or(RepositoryError::CorruptState)?;
    ref_current(db, &task, &r, ReferenceValidation::ActiveRead)
}

#[cfg(test)]
#[path = "task_workspace_output_tests.rs"]
mod output_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BranchId, CreateTaskRequest, ProjectId, ProjectRecord, SnapshotId};
    fn fixture() -> (SqliteStoryRepository, TaskRecord) {
        fixture_with_intent("检查远章线索。")
    }
    fn fixture_with_intent(author_intent: &str) -> (SqliteStoryRepository, TaskRecord) {
        let mut repo = SqliteStoryRepository::in_memory().unwrap();
        let p = ProjectId::parse("project").unwrap();
        repo.create_project(
            ProjectRecord {
                project_id: p.clone(),
                title: "长篇".into(),
                logline: "".into(),
                created_at: 1,
                updated_at: 1,
            },
            BranchId::parse("main").unwrap(),
            SnapshotId::parse("initial").unwrap(),
        )
        .unwrap();
        let tx = repo.connection.transaction().unwrap();
        for n in 1..=1000 {
            let chapter = format!("chapter-{n}");
            let object = format!("draft-{chapter}");
            let revision = format!("revision-{n}");
            let body = if n == 7 {
                "远章的真实线索。".repeat(1500)
            } else {
                "其余正文。".repeat(1500)
            };
            tx.execute(
                "INSERT INTO story_chapters_v2 VALUES(?1,'project',?2,?3,'candidate','')",
                params![chapter, n, format!("第{n}章")],
            )
            .unwrap();
            tx.execute("INSERT INTO story_revisions_v2 VALUES(?1,'project',?2,'draft',NULL,'digest',?3)",params![revision,object,json!({"chapterId":chapter,"chapterNumber":n,"title":format!("第{n}章"),"body":body,"sceneGroups":[{"id":"first","body":body}]}).to_string()]).unwrap();
            tx.execute(
                "INSERT INTO story_object_heads_v2 VALUES('project',?1,'draft',?2,NULL,0)",
                params![object, revision],
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let task = repo
            .create_task(CreateTaskRequest {
                task_id: "reader".into(),
                project_id: p,
                thread_id: "root-thread".into(),
                operation: "advisory".into(),
                input_revision: 0,
                model: None,
                target_json: json!({"type":"project"}).to_string(),
                input_manifest_json: json!({"authorIntent":author_intent}).to_string(),
                capability_hash: "a".repeat(64),
                capability_expires_at: i64::MAX,
            })
            .unwrap();
        (repo, task)
    }
    fn directory(repo: &SqliteStoryRepository, t: &TaskRecord) -> String {
        let mut context = json!({"coverage":{"complete":false,"missingScope":[]}});
        repo.project_task_sources(t, &mut context).unwrap();
        assert!(context.to_string().len() < 6500);
        context["taskWorkspace"]["sourceId"]
            .as_str()
            .unwrap()
            .into()
    }
    #[test]
    fn scene_expansion_preserves_full_text_pagination_and_rejects_other_groups() {
        let (repo, task) = fixture();
        let body = "场景中的线索🙂".repeat(6000);
        let content = json!({"chapterId":"chapter-7","sceneGroups":[
            {"id":"first","body":body},{"id":"second","body":"另一场景不可混入"}]});
        repo.connection.execute("UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='revision-7'",[content.to_string()]).unwrap();
        let page = repo.task_workspace_page(&task,None,None,Some(7),Some("draft"),None,None).unwrap();
        let source = page["items"][0]["sourceId"].as_str().unwrap();
        let first = repo.task_source_scene(&task,source,"first",None,"child").unwrap();
        assert!(first["nextCursor"].is_string());
        assert!(!first.to_string().contains("另一场景不可混入"));
        assert!(repo.task_source_scene(&task,source,"second",first["nextCursor"].as_str(),"child").is_err());
        assert!(repo.task_source_scene(&task,source,"missing",None,"child").is_err());
        let mut text = String::new();
        let mut page = first;
        loop {
            for entry in page["entries"].as_array().unwrap() {
                if entry["path"] == "/content/sceneGroups/0/body" {
                    text.push_str(entry["text"].as_str().or(entry["value"].as_str()).unwrap());
                }
            }
            let Some(cursor) = page["nextCursor"].as_str() else { break; };
            page = repo.task_source_scene(&task,source,"first",Some(cursor),"child").unwrap();
        }
        assert_eq!(text,body);
        let n:i64=repo.connection.query_row("SELECT COUNT(*) FROM story_task_source_returned_v1",[],|r|r.get(0)).unwrap();
        assert_eq!(n,0,"child scene reads do not certify root full reads");
    }
    #[test]
    fn partial_prose_reads_track_version_dependencies_without_full_read_credit() {
        for locate in [false,true] {
            let (repo, task) = fixture();
            let page = repo.task_workspace_page(&task,None,None,Some(7),Some("draft"),None,None).unwrap();
            let source = page["items"][0]["sourceId"].as_str().unwrap();
            if locate {
                repo.task_source_locate_for_reader(&task,source,"远章的真实线索",Some("first"),None,"child").unwrap();
            } else {
                repo.task_workspace_passages_for_reader(&task,None,None,Some(7),None,None,"end",80,"child").unwrap();
            }
            let readers:Vec<String> = repo.connection.prepare("SELECT reader_thread_id FROM story_task_workspace_reads_v1").unwrap()
                .query_map([],|r|r.get(0)).unwrap().map(Result::unwrap).collect();
            assert_eq!(readers,vec!["child"]);
            assert!(dependencies_current(&repo.connection,&task.task_id).unwrap());
            repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='changed' WHERE object_id='draft-chapter-7'",[]).unwrap();
            assert!(!dependencies_current(&repo.connection,&task.task_id).unwrap());
            let n:i64=repo.connection.query_row("SELECT COUNT(*) FROM story_task_source_returned_v1",[],|r|r.get(0)).unwrap();
            assert_eq!(n,0);
        }
    }
    #[test]
    fn passages_batch_exact_frozen_text_without_full_read_credit_and_bind_all_filters() {
        let (repo, task) = fixture();
        let first = repo.task_workspace_passages(&task,None,None,None,None,None,"end",80).unwrap();
        assert_eq!(first["representation"],"workspacePassages");
        assert_eq!(first["selection"],"frozenTaskBaseline");
        assert_eq!(first["coverage"]["completeSource"],false);
        assert!(first["items"].as_array().unwrap().len()>1);
        assert!(first.to_string().len()<6500);
        for item in first["items"].as_array().unwrap() {
            let source=item["sourceId"].as_str().unwrap();
            let (_,hash,raw)=repo.materialize_workspace_source(&task,source).unwrap().unwrap();
            assert_eq!(item["contentHash"],hash);
            let value:Value=serde_json::from_str(&raw).unwrap();
            let text=value.pointer(item["excerpt"]["path"].as_str().unwrap()).unwrap().as_str().unwrap();
            assert_eq!(item["excerpt"]["text"],text.chars().skip(text.chars().count()-80).collect::<String>());
        }
        let cursor=first["nextCursor"].as_str().unwrap();
        let next=repo.task_workspace_passages(&task,None,Some(cursor),None,None,None,"end",80).unwrap();
        assert!(next["items"][0]["chapterNumber"].as_i64().unwrap()>first["items"].as_array().unwrap().last().unwrap()["chapterNumber"].as_i64().unwrap());
        for (edge,count) in [("start",80),("end",81)] {
            assert!(repo.task_workspace_passages(&task,None,Some(cursor),None,None,None,edge,count).is_err());
        }
        assert!(repo.task_workspace_page(&task,None,Some(cursor),None,Some("draft"),None,None).is_err());
        assert!(repo.task_workspace_passages(&task,None,None,None,None,None,"end",513).is_err());
        assert!(repo.task_workspace_passages(&task,Some(&format!("source-{}","f".repeat(64))),None,None,None,None,"end",80).is_err());
        let absent=repo.task_workspace_passages(&task,None,None,Some(7),None,Some("foreign-revision"),"end",80).unwrap();
        assert!(absent["items"].as_array().unwrap().is_empty());
        for table in ["story_task_source_returned_v1","story_task_source_raw_returns_v1"] {
            let n:i64=repo.connection.query_row(&format!("SELECT COUNT(*) FROM {table}"),[],|r|r.get(0)).unwrap();
            assert_eq!(n,0,"partial excerpts must not certify complete source reads");
        }
    }
    #[test]
    fn passages_do_not_expose_future_pov_or_scene_suffixes() {
        let (repo,mut task)=fixture();
        task.operation="generateChapter".into();
        task.target_json=json!({"chapterId":"chapter-7","chapterNumber":7,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},"readScope":{"type":"throughScene","endSceneOrder":1},"povCharacterId":"character"}).to_string();
        // This isolated visibility fixture changes the task kind; regenerate the
        // production frozen method binding instead of leaving its digest stale.
        let mut manifest: Value = serde_json::from_str(&task.input_manifest_json).unwrap();
        manifest.as_object_mut().unwrap().remove("creativeCapabilityBinding");
        crate::creative_methods::freeze(&repo.connection, &CreateTaskRequest {
            task_id:task.task_id.clone(),project_id:task.project_id.clone(),thread_id:task.thread_id.clone(),
            operation:task.operation.clone(),input_revision:task.input_revision,model:None,
            target_json:task.target_json.clone(),input_manifest_json:manifest.to_string(),
            capability_hash:task.capability_hash.clone(),capability_expires_at:task.capability_expires_at,
        }, &serde_json::from_str(&task.target_json).unwrap(), &mut manifest).unwrap();
        task.input_manifest_json = manifest.to_string();
        repo.connection.execute("UPDATE story_tasks_v2 SET operation=?1,target_json=?2,input_manifest_json=?4 WHERE task_id=?3",params![task.operation,task.target_json,task.task_id,task.input_manifest_json]).unwrap();
        let content=json!({"chapterId":"chapter-7","body":"visible hidden","sceneGroups":[{"id":"g1","body":"visible","locked":true},{"id":"g2","body":"hidden","locked":false}]});
        repo.connection.execute("UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='revision-7'",[content.to_string()]).unwrap();
        repo.source_task(&task).expect("active frozen task");
        scope(&repo, &task).expect("visible scope");
        let page=repo.task_workspace_passages(&task,None,None,None,None,None,"end",80).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(),1);
        assert_eq!(page["items"][0]["chapterNumber"],7);
        assert_eq!(page["items"][0]["excerpt"]["text"],"visible");
        assert_eq!(page["visibility"]["prose"]["throughScene"],1);
        assert!(!page.to_string().contains("hidden"));
    }
    #[test]
    fn current_task_locator_filters_revision_without_read_credit_or_foreign_binding() {
        let (repo, task) = fixture();
        let located = repo.task_workspace_page(&task, None, None, Some(7), Some("draft"), None, Some("revision-7")).unwrap();
        assert_eq!(located["items"].as_array().unwrap().len(), 1);
        assert_eq!(located["items"][0]["revisionId"], "revision-7");
        assert_eq!(located["contentRead"], false);
        let read_count: i64 = repo.connection.query_row("SELECT COUNT(*) FROM story_task_workspace_reads_v1", [], |r| r.get(0)).unwrap();
        assert_eq!(read_count, 0);
        for revision in ["revision-8", "foreign-revision"] {
            let result = repo.task_workspace_page(&task, None, None, Some(7), Some("draft"), None, Some(revision)).unwrap();
            assert!(result["items"].as_array().unwrap().is_empty());
        }
        assert!(repo.task_workspace_page(&task, Some(&format!("source-{}", "f".repeat(64))), None, Some(7), None, None, None).is_err());
        let page = repo.task_workspace_page(&task, None, None, None, Some("draft"), None, None).unwrap();
        assert!(repo.task_workspace_page(&task, None, page["nextCursor"].as_str(), None, Some("draft"), None, Some("revision-7")).is_err());
    }

    #[test]
    fn thousand_chapter_directory_is_bounded_lazy_and_far_text_is_exact() {
        let (repo, t) = fixture();
        let source = directory(&repo, &t);
        let cached: i64 = repo
            .connection
            .query_row(
                "SELECT SUM(length(CAST(payload_json AS BLOB))) FROM story_task_source_items_v2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            cached < 3000,
            "catalogue must not copy 1000 chapter bodies: {cached}"
        );
        let first = repo
            .task_workspace_page(&t, Some(&source), None, None, None, None, None)
            .unwrap();
        assert!(first.to_string().len() <= 6500);
        assert_eq!(first["items"].as_array().unwrap().len(), 8);
        assert_eq!(first["contentRead"], false);
        assert!(first["nextCursor"].is_string());
        let far = repo
            .task_workspace_page(&t, Some(&source), None, Some(7), Some("draft"), None, None)
            .unwrap();
        let leaf = far["items"][0]["sourceId"].as_str().unwrap();
        assert_eq!(far["items"][0]["revisionId"], "revision-7");
        assert_eq!(far["items"][0]["contentHash"], Value::Null);
        let parsed = repo.task_source_content(&t, leaf, None).unwrap();
        assert!(parsed.to_string().len() <= crate::task_sources::SOURCE_CONTENT_PAGE_BYTES);
        let text = parsed.to_string();
        assert!(text.contains("远章的真实线索"));
        let loaded:i64=repo.connection.query_row("SELECT COUNT(*) FROM story_task_source_items_v2 WHERE policy_digest='workspaceObject'",[],|r|r.get(0)).unwrap();
        assert_eq!(loaded, 1);
        let r = load_ref(&repo.connection, &t.task_id, leaf)
            .unwrap()
            .unwrap();
        assert!(ref_current(&repo.connection, &t, &r, ReferenceValidation::ActiveRead).unwrap());
        repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='unrelated' WHERE object_id='draft-chapter-999'",[]).unwrap();
        assert!(dependencies_current(&repo.connection, &t.task_id).unwrap());
        repo.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='changed' WHERE object_id='draft-chapter-7'",[]).unwrap();
        assert!(!dependencies_current(&repo.connection, &t.task_id).unwrap());
    }
    #[test]
    fn child_pages_are_attributed_without_certifying_root_and_filters_bind_cursor() {
        let (repo, t) = fixture();
        let source = directory(&repo, &t);
        let page = repo
            .task_workspace_page(&t, Some(&source), None, None, None, Some("没有命中"), None)
            .unwrap();
        assert_eq!(page["coverage"]["scannedRecords"], 64);
        assert!(page["nextCursor"].is_string());
        assert!(
            repo.task_workspace_page(&t, Some(&source), page["nextCursor"].as_str(), Some(7), None, Some("没有命中"), None)
            .is_err()
        );
        let far = repo
            .task_workspace_page(&t, Some(&source), None, Some(7), None, None, None)
            .unwrap();
        let leaf = far["items"][0]["sourceId"].as_str().unwrap();
        repo.task_source_content_for_reader(&t, leaf, None, "trusted-child")
            .unwrap();
        let root: i64 = repo
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_task_source_returned_v1 WHERE source_id=?1",
                [leaf],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(root, 0);
        let reader: String = repo
            .connection
            .query_row(
                "SELECT reader_thread_id FROM story_task_workspace_reads_v1 WHERE source_id=?1",
                [leaf],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reader, "trusted-child");
        repo.task_source_content(&t, leaf, None).unwrap();
        let root: i64 = repo
            .connection
            .query_row(
                "SELECT COUNT(*) FROM story_task_source_returned_v1 WHERE source_id=?1",
                [leaf],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(root, 1);
        let mut foreign = t.clone();
        foreign.task_id = "another-task".into();
        assert!(
            repo.materialize_workspace_source(&foreign, leaf)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn writer_visibility_keeps_future_pov_and_unselected_suffix_hidden() {
        let (repo, mut t) = fixture();
        t.operation = "generateChapter".into();
        t.target_json=json!({"chapterId":"chapter-7","chapterNumber":7,"sceneOrder":1,"scope":{"type":"throughScene","endSceneOrder":1},"readScope":{"type":"throughScene","endSceneOrder":1},"povCharacterId":"character"}).to_string();
        let s = scope(&repo, &t).unwrap();
        assert_eq!((s.from, s.through, s.artifacts), (7, 7, false));
        let r = Ref {
            object: "draft-chapter-7".into(),
            revision: "revision-7".into(),
            purpose: "currentDraft".into(),
            authority: "candidate".into(),
            selection: None,
            end_scene: Some(1),
            number: Some(7),
            title: None,
            producer: None,
            ordinal: None,
        };
        let id = r.register(&repo.connection, &t).unwrap()["sourceId"]
            .as_str()
            .unwrap()
            .to_owned();
        let full = json!({"body":"visible\n\n---\n\nhidden","sceneGroups":[{"body":"visible"},{"body":"hidden"}]});
        repo.connection
            .execute(
                "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='revision-7'",
                [full.to_string()],
            )
            .unwrap();
        let (_, _, text) = repo.materialize_workspace_source(&t, &id).unwrap().unwrap();
        assert!(text.contains("visible"));
        assert!(!text.contains("hidden"));
    }
    #[test]
    fn original_author_standards_require_complete_root_read_and_exact_current_quotes() {
        let intent = format!(
            "{}依据作者原话保留关键转折。",
            "作者要求与完整约束。".repeat(crate::task_sources::SOURCE_CONTENT_PAGE_BYTES / "作者要求与完整约束。".len() * 2)
        );
        let (repo, t) = fixture_with_intent(&intent);
        assert!(
            crate::task_sources::require_author_requirements_read(&repo.connection, &t).is_err()
        );
        directory(&repo, &t);
        let(source,hash):(String,String)=repo.connection.query_row("SELECT source_id,content_hash FROM story_task_source_items_v2 WHERE task_id=?1 AND section_path='/authorRequirementsSource'",[&t.task_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        let work:String=repo.connection.query_row("SELECT s.work_task_id FROM story_work_attempts_v1 a JOIN story_work_steps_v1 s ON s.id=a.step_id WHERE a.attempt_id=?1",[&t.task_id],|r|r.get(0)).unwrap();
        let quote = "依据作者原话保留关键转折。";
        let check = || {
            crate::task_sources::validate_author_requirement_quote(
                &repo.connection,
                &t.project_id,
                &work,
                &source,
                &hash,
                quote,
            )
        };
        assert!(check().is_err());
        repo.task_source_index(&t, &source, None).unwrap();
        assert!(check().is_err());
        let mut cursor = None;
        loop {
            let page = repo
                .task_source_content_for_reader(&t, &source, cursor.as_deref(), "trusted-child")
                .unwrap();
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        assert!(
            crate::task_sources::require_author_requirements_read(&repo.connection, &t).is_err()
        );
        assert!(check().is_err());
        let first = repo.task_source_content(&t, &source, None).unwrap();
        assert!(first["nextCursor"].is_string());
        assert!(
            crate::task_sources::require_author_requirements_read(&repo.connection, &t).is_err()
        );
        let mut cursor = first["nextCursor"].as_str().map(str::to_owned);
        while let Some(c) = cursor {
            let page = repo.task_source_content(&t, &source, Some(&c)).unwrap();
            cursor = page["nextCursor"].as_str().map(str::to_owned);
        }
        assert!(
            crate::task_sources::require_author_requirements_read(&repo.connection, &t).is_ok()
        );
        assert!(check().is_ok());
        assert!(
            crate::task_sources::validate_author_requirement_quote(
                &repo.connection,
                &t.project_id,
                "foreign-work",
                &source,
                &hash,
                quote
            )
            .is_err()
        );
        assert!(
            crate::task_sources::validate_author_requirement_quote(
                &repo.connection,
                &t.project_id,
                &work,
                &source,
                &"b".repeat(64),
                quote
            )
            .is_err()
        );
        assert!(
            crate::task_sources::validate_author_requirement_quote(
                &repo.connection,
                &t.project_id,
                &work,
                &source,
                &hash,
                "改写成另一条要求"
            )
            .is_err()
        );
    }

use crate::{ProjectRevisionRepository, TaskSourcesRepository, TaskWorkspaceRepository, TasksRepository};
}

/// Public task workspace capability; SQLite retains transaction ownership.
pub trait TaskWorkspaceRepository {
    /// Locate this task's actual committed output without a model directory scan.
    /// This registers a reference only; it does not deliver text or reading evidence.
    fn task_saved_draft_source(
        &self,
        task: &TaskRecord,
        revision: &str,
    ) -> Result<Value, RepositoryError>;
    /// Directory/search response is bounded independently of the book's chapter count.
    /// Search is a literal locator over admitted records, not a fact/absence decision.
    fn task_workspace_page(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        object_kind: Option<&str>,
        query: Option<&str>,
        revision_id: Option<&str>,
    ) -> Result<Value, RepositoryError>;
    /// Batched partial original prose from the task's frozen visible baseline.
    fn task_workspace_passages(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        query: Option<&str>,
        revision_id: Option<&str>,
        edge: &str,
        characters: usize,
    ) -> Result<Value, RepositoryError>;
    fn task_workspace_passages_for_reader(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        query: Option<&str>,
        revision_id: Option<&str>,
        edge: &str,
        characters: usize,
        reader: &str,
    ) -> Result<Value, RepositoryError>;
}

impl TaskWorkspaceRepository for SqliteStoryRepository {
    /// Locate this task's actual committed output without a model directory scan.
    /// This registers a reference only; it does not deliver text or reading evidence.
    fn task_saved_draft_source(
        &self,
        task: &TaskRecord,
        revision: &str,
    ) -> Result<Value, RepositoryError> {
        let t = target(task)?;
        let s = scope(self, task)?;
        let chapter = t["chapterId"]
            .as_str()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        let number = t["chapterNumber"]
            .as_i64()
            .ok_or(RepositoryError::TaskBindingInvalid)?;
        if number < s.from || number > s.through {
            return Err(RepositoryError::TaskSourceNotInCatalog);
        }
        let ordinal: i64 = self.connection.query_row(
            "SELECT ordinal FROM story_task_outputs_v2 WHERE task_id=?1 AND revision_id=?2 AND output_kind='draft'",
            params![task.task_id, revision], |r| r.get(0),
        ).optional().map_err(storage)?.ok_or(RepositoryError::TaskSourceNotInCatalog)?;
        let reference = Ref {
            object: format!("draft-{chapter}"),
            revision: revision.into(),
            purpose: "savedCheckpoint".into(),
            authority: "nativeSavedCheckpoint".into(),
            selection: None,
            end_scene: if number == s.through {
                s.end_scene
            } else {
                None
            },
            number: Some(number),
            title: None,
            producer: Some(task.task_id.clone()),
            ordinal: Some(ordinal),
        };
        if !ref_current(
            &self.connection,
            task,
            &reference,
            ReferenceValidation::ActiveRead,
        )? {
            return Err(RepositoryError::TaskSourceNotInCatalog);
        }
        reference.register(&self.connection, task)
    }

    /// Directory/search response is bounded independently of the book's chapter count.
    /// Search is a literal locator over admitted records, not a fact/absence decision.
    fn task_workspace_page(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        object_kind: Option<&str>,
        query: Option<&str>,
        revision_id: Option<&str>,
    ) -> Result<Value, RepositoryError> {
        self.workspace_page(
            task,
            source,
            cursor,
            chapter_number,
            object_kind,
            query,
            revision_id,
            None,
            &task.thread_id,
        )
    }

    /// Batched partial original prose from the task's frozen visible baseline.
    fn task_workspace_passages(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        query: Option<&str>,
        revision_id: Option<&str>,
        edge: &str,
        characters: usize,
    ) -> Result<Value, RepositoryError> {
        self.task_workspace_passages_for_reader(
            task,
            source,
            cursor,
            chapter_number,
            query,
            revision_id,
            edge,
            characters,
            &task.thread_id,
        )
    }

    fn task_workspace_passages_for_reader(
        &self,
        task: &TaskRecord,
        source: Option<&str>,
        cursor: Option<&str>,
        chapter_number: Option<i64>,
        query: Option<&str>,
        revision_id: Option<&str>,
        edge: &str,
        characters: usize,
        reader: &str,
    ) -> Result<Value, RepositoryError> {
        if !matches!(edge, "start" | "end") || !(1..=512).contains(&characters) {
            return Err(RepositoryError::InvalidContent(
                "passages requires edge=start/end and characters=1..512".into(),
            ));
        }
        self.workspace_page(
            task,
            source,
            cursor,
            chapter_number,
            Some("draft"),
            query,
            revision_id,
            Some((edge, characters)),
            reader,
        )
    }
}
