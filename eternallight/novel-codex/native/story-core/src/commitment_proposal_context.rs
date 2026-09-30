use rusqlite::params;
use serde_json::{Value, json};

use crate::query_scope::TaskQueryScope;
use crate::{
    CommitmentStatus, CommitmentWindow, ObjectId, RepositoryError, SearchScope,
    SqliteStoryRepository, TaskRecord,
};

const ENTRY_LIMIT: usize = 12;
const BYTE_LIMIT: usize = 8_000;
const ENTRY_BYTE_LIMIT: usize = 2_000;
const SCAN_LIMIT: usize = 100;

fn storage(error: rusqlite::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

impl SqliteStoryRepository {
    pub(crate) fn select_commitment_proposals(
        &self,
        task: &TaskRecord,
        scope: &TaskQueryScope,
    ) -> Result<Value, RepositoryError> {
        if !matches!(
            task.operation.as_str(),
            "generateBrief" | "generateOutline" | "generatePlan" | "reviewPlanningSeat"
        ) {
            return Ok(
                json!({"entries":[],"coverage":{"requested":false,"complete":true,"includedEntries":0}}),
            );
        }
        let (first, last) = match scope.text_scope() {
            SearchScope::All => (1, i64::MAX),
            SearchScope::Chapter(chapter) => (chapter, chapter),
            SearchScope::ThroughChapter(chapter) => (1, chapter),
            SearchScope::ThroughScene { .. } => return Err(RepositoryError::TaskBindingInvalid),
        };
        // Source currency, authorization and author establishment precede LIMIT.
        // A later author edit must never be undone by reintroducing its origin proposal.
        let mut statement = self.connection.prepare(
            "SELECT r.revision_id,json_extract(r.content_json,'$.payload.chapterId'),
                s.revision_id,json_extract(s.content_json,'$.chapterNumber'),p.value
             FROM story_branches_v2 b JOIN story_snapshot_members_v2 m ON m.snapshot_id=b.head_snapshot_id
             JOIN story_revisions_v2 r ON r.project_id=b.project_id AND r.revision_id=m.revision_id
             JOIN story_snapshot_members_v2 sm ON sm.snapshot_id=b.head_snapshot_id
                AND sm.object_id='draft-' || json_extract(r.content_json,'$.payload.chapterId')
             JOIN story_revisions_v2 s ON s.project_id=b.project_id AND s.revision_id=sm.revision_id
                AND s.object_kind='draft' AND s.revision_id=json_extract(r.content_json,'$.payload.sourceRevisionId')
             JOIN json_each(r.content_json,'$.payload.candidates') p
             WHERE b.project_id=?1 AND b.branch_id='main' AND r.object_kind='commitmentExtraction'
                AND json_extract(s.content_json,'$.chapterNumber') BETWEEN ?2 AND ?3
                AND NOT EXISTS (
                    SELECT 1 FROM story_snapshot_members_v2 cm
                    JOIN story_revisions_v2 c ON c.project_id=b.project_id AND c.revision_id=cm.revision_id
                    WHERE cm.snapshot_id=b.head_snapshot_id AND c.object_kind='commitment'
                      AND json_extract(c.content_json,'$.extractionOrigin.reportRevisionId')=r.revision_id
                      AND json_extract(c.content_json,'$.extractionOrigin.proposalId')=json_extract(p.value,'$.id')
                )
             ORDER BY json_extract(s.content_json,'$.chapterNumber') DESC,r.revision_id,json_extract(p.value,'$.id')
             LIMIT ?4"
        ).map_err(storage)?;
        let mut rows = statement
            .query(params![
                task.project_id.as_str(),
                first,
                last,
                SCAN_LIMIT + 1
            ])
            .map_err(storage)?;
        let mut entries = Vec::new();
        let mut bytes = 2;
        let mut scanned = 0;
        let mut truncated = false;
        let mut oversized = false;
        while let Some(row) = rows.next().map_err(storage)? {
            if scanned == SCAN_LIMIT || entries.len() == ENTRY_LIMIT {
                truncated = true;
                break;
            }
            scanned += 1;
            let revision: String = row.get(0).map_err(storage)?;
            let chapter: String = row.get(1).map_err(storage)?;
            let source: String = row.get(2).map_err(storage)?;
            let number: i64 = row.get(3).map_err(storage)?;
            let candidate: Value = serde_json::from_str(&row.get::<_, String>(4).map_err(storage)?)
                .map_err(|_| RepositoryError::CorruptState)?;
            let id = candidate["id"]
                .as_str()
                .ok_or(RepositoryError::CorruptState)?;
            let value = crate::narrative_commitments::validate_content(
                &self.connection,
                &task.project_id,
                &ObjectId::parse(format!("commitment-{id}"))
                    .map_err(|_| RepositoryError::CorruptState)?,
                &json!({"payload":candidate}).to_string(),
            )?;
            if !matches!(value.status, CommitmentStatus::Open)
                || !matches!(value.target_window, CommitmentWindow::Open)
                || value.decision_reason.is_some()
                || value.setup_evidence_refs.iter().any(|anchor| {
                    anchor.revision_id != source || anchor.object_id != format!("draft-{chapter}")
                })
            {
                return Err(RepositoryError::CorruptState);
            }
            let projection = json!({
                "chapterId":chapter,"chapterNumber":number,"reportRevisionId":revision,
                "sourceRevisionId":source,"proposalId":value.id,"kind":value.kind,
                "summary":value.summary,"fulfillmentCriteria":value.fulfillment_criteria,
                "setupEvidenceRefs":value.setup_evidence_refs,"authority":"candidateProposal",
                "established":false,"confirmationState":"unconfirmed","sourceCurrent":true,
                "literaryFulfillmentVerified":false
            });
            let size = projection.to_string().len();
            if size > ENTRY_BYTE_LIMIT {
                oversized = true;
                truncated = true;
                continue;
            }
            let cost = size + usize::from(!entries.is_empty());
            if bytes + cost > BYTE_LIMIT {
                truncated = true;
                continue;
            }
            bytes += cost;
            entries.push(projection);
        }
        Ok(json!({"entries":entries,"coverage":{
            "requested":true,"complete":!truncated,"truncated":truncated,"oversizedEntriesOmitted":oversized,
            "includedEntries":entries.len(),"scannedCandidates":scanned,"sourceCurrentOnly":true,
            "semanticCompleteness":"unknown","globalCompletenessClaimed":false,
            "entryLimit":ENTRY_LIMIT,"byteLimit":BYTE_LIMIT,"entryByteLimit":ENTRY_BYTE_LIMIT,
            "scanLimit":SCAN_LIMIT,"selectionPolicy":"recentChapterThenReportAndProposalId"
        }}))
    }
}
