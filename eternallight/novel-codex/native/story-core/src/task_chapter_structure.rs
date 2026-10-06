use crate::{RepositoryError, SqliteStoryRepository, TaskRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChapterStructureSourceRole {
    Saved,
    Accepted,
    Working,
}
impl ChapterStructureSourceRole {
    fn field(self) -> &'static str {
        match self {
            Self::Saved => "savedRevisionId",
            Self::Accepted => "acceptedRevisionId",
            Self::Working => "workingRevisionId",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChapterStructureSourceRequest {
    pub thread_id: String,
    pub expected_revision: i64,
    pub source_role: ChapterStructureSourceRole,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

impl SqliteStoryRepository {
    /// Immutable removed source is reachable only through its funded, active assessment Work.
    pub fn chapter_structure_context(
        &self,
        task: &TaskRecord,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        self.with_read_snapshot(|| {
            let task = self.authorize_task_capability(
                &task.project_id,
                &task.task_id,
                &task.thread_id,
                task.input_revision,
                &task.capability_hash,
                now,
            )?;
            self.assert_task_inputs_current(&task)?;
            let mut event = crate::chapter_structure::assert_task(&self.connection, &task)?;
            event
                .as_object_mut()
                .ok_or(RepositoryError::CorruptState)?
                .remove("assessment");
            event["authority"] = json!("removedHistoricalSource");
            event["coverage"] = json!({"globalImpactComplete":false,"sourceTextDelivered":false});
            if event.to_string().len() > 4096 {
                return Err(RepositoryError::InvalidContent(
                    "structure_metadata_budget_exceeded".into(),
                ));
            }
            Ok(event)
        })?
    }

    pub fn read_chapter_structure_source(
        &self,
        task: &TaskRecord,
        request: &ChapterStructureSourceRequest,
        now: i64,
    ) -> Result<Value, RepositoryError> {
        let limit = request.limit.unwrap_or(600);
        if !(1..=1000).contains(&limit)
            || request.thread_id != task.thread_id
            || request.expected_revision != task.input_revision
            || request.cursor.as_ref().is_some_and(|c| c.len() > 96)
        {
            return Err(RepositoryError::TaskBindingInvalid);
        }
        self.with_read_snapshot(||{
            let task=self.authorize_task_capability(&task.project_id,&task.task_id,&task.thread_id,task.input_revision,&task.capability_hash,now)?;
            self.assert_task_inputs_current(&task)?;
            let event=crate::chapter_structure::assert_task(&self.connection,&task)?;
            let revision=event["source"][request.source_role.field()].as_str().ok_or_else(||RepositoryError::InvalidContent("structure_source_unavailable".into()))?;
            let object=event["source"]["objectId"].as_str().ok_or(RepositoryError::CorruptState)?;
            let content=crate::chapter_structure::source_content(&self.connection,&task.project_id,object,revision)?;
            let body=content["body"].as_str().ok_or(RepositoryError::CorruptState)?;
            let total=body.chars().count();
            let signature=|offset:usize|format!("{:x}",Sha256::digest(json!([task.capability_hash,task.task_id,task.project_id.as_str(),task.input_revision,task.input_manifest_json,event["id"],request.source_role,revision,offset]).to_string().as_bytes()));
            let start=match request.cursor.as_deref() {
                None=>0,
                Some(cursor)=>{
                    let (offset,proof)=cursor.split_once('.').ok_or(RepositoryError::TaskBindingInvalid)?;
                    let offset=offset.parse::<usize>().map_err(|_|RepositoryError::TaskBindingInvalid)?;
                    if proof!=signature(offset) || offset>total {return Err(RepositoryError::TaskBindingInvalid);}
                    offset
                }
            };
            let end=start.saturating_add(limit).min(total);
            let quote=body.chars().skip(start).take(end-start).collect::<String>();
            let next=if end<total {Some(format!("{end}.{}",signature(end)))} else {None};
            let report=json!({"projectId":task.project_id.as_str(),"taskId":task.task_id,"structureChangeId":event["id"],"sourceRole":request.source_role,
                "source":{"objectId":object,"revisionId":revision,"sourcePath":"/body"},"authority":"removedHistoricalSource",
                "start":start,"end":end,"total":total,"quote":quote,"nextCursor":next,"truncated":false,
                "coverage":{"globalImpactComplete":false,"endOfSourceFromCursor":end==total}});
            // Check the escaped MCP wrapper, not just raw text. No partial page is delivered.
            let wrapped=json!({"content":[{"type":"text","text":report.to_string()}],"isError":false});
            if wrapped.to_string().len()>9000 {return Err(RepositoryError::InvalidContent("structure_source_budget_exceeded".into()));}
            Ok(report)
        })?
    }
}
