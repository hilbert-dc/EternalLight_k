use super::*;
use crate::{BranchId, CreateTaskRequest, ProjectRecord, SnapshotId};
fn fixture() -> (SqliteStoryRepository, ProjectId) {
    let mut repo = SqliteStoryRepository::in_memory().unwrap();
    let p = ProjectId::parse("quality-book").unwrap();
    repo.create_project(
        ProjectRecord {
            project_id: p.clone(),
            title: "雨城".into(),
            logline: "".into(),
            created_at: 1,
            updated_at: 1,
        },
        BranchId::parse("main").unwrap(),
        SnapshotId::parse("initial").unwrap(),
    )
    .unwrap();
    repo.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('draft-a',?1,'draft-chapter','draft','digest',?2)",params![p.as_str(),json!({"body":"她说不必再来，却将那把伞留在门边。\n\n雨水滴进空碗。"}).to_string()]).unwrap();
    repo.connection.execute("INSERT INTO story_object_heads_v2(project_id,object_id,object_kind,candidate_revision_id) VALUES(?1,'draft-chapter','draft','draft-a')",[p.as_str()]).unwrap();
    (repo, p)
}
fn evidence() -> Vec<QualityEvidence> {
    vec![QualityEvidence {
        revision_id: "draft-a".into(),
        quote: "却将那把伞留在门边".into(),
        role: "current".into(),
    }]
}

#[test]
fn autonomous_packets_are_work_owned_current_optional_and_not_replayed() {
    let (mut r, p) = fixture();
    let parent = task(&mut r, &p, "advisory", json!({}), "parent");
    let other = task(&mut r, &p, "advisory", json!({}), "other");
    let make = || QualityAction::Packet {
        kind: "reading".into(),
        sources: vec![QualityPacketSource {
            revision_id: Some("draft-a".into()),
            quote: None,
            note_id: None,
        }],
    };
    // An author packet is not ambient authority to spend any Work's budget.
    change(&mut r, &p, make());
    let receipt = r
        .change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 1,
                idempotency_key: "auto-packet".into(),
                change: make(),
            },
            Some(&parent),
        )
        .unwrap();
    let id = receipt["result"]["id"].as_str().unwrap();
    let mut ready = Vec::new();
    reader_actions(&r.connection, &p, "work-parent", &mut ready).unwrap();
    assert_eq!(ready.len(), 1);
    assert!(!ready[0].required);
    assert_eq!(ready[0].target["qualityPacketId"], id);
    let mut unrelated = Vec::new();
    reader_actions(
        &r.connection,
        &p,
        &format!("work-{}", other.task_id),
        &mut unrelated,
    )
    .unwrap();
    assert!(unrelated.is_empty());
    r.connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id=?1",
            [p.as_str()],
        )
        .unwrap();
    let mut stale = Vec::new();
    reader_actions(&r.connection, &p, "work-parent", &mut stale).unwrap();
    assert!(stale.is_empty());
    r.connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id='draft-a' WHERE project_id=?1",
            [p.as_str()],
        )
        .unwrap();
    let reader = task(
        &mut r,
        &p,
        "reviewQuality",
        json!({"type":"project","qualityPacketId":id}),
        "reader",
    );
    let mut running = Vec::new();
    reader_actions(&r.connection, &p, "work-parent", &mut running).unwrap();
    assert!(running.is_empty());
    assert_eq!(reader.operation, "reviewQuality");
}

#[test]
fn creative_workers_can_read_preferences_but_cannot_write_quality_records() {
    let (mut r, p) = fixture();
    change(&mut r, &p, QualityAction::Note { note: item("preference") });
    for operation in ["generateBrief", "generateOutline", "generatePlan", "generateChapter"] {
        let worker = task(&mut r, &p, operation, if operation == "generateChapter" { json!({"chapterId":"chapter"}) } else { json!({}) }, operation);
        assert_eq!(r.quality_notebook(&p, None, Some(&worker)).unwrap()["notes"][0]["status"], "confirmed");
        assert!(r.change_quality_notebook(&p, QualityMutation { expected_version: 1, idempotency_key: operation.into(), change: QualityAction::Note { note: item("strength") } }, Some(&worker)).is_err());
    }
}

#[test]
fn autonomous_quality_round_trip_uses_original_work_and_returns_report_to_decision() {
    let (mut r, p) = fixture();
    let root = task(&mut r, &p, "advisory", json!({}), "auto-root");
    let receipt = r
        .change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 0,
                idempotency_key: "auto".into(),
                change: QualityAction::Packet {
                    kind: "reading".into(),
                    sources: vec![QualityPacketSource {
                        revision_id: Some("draft-a".into()),
                        quote: None,
                        note_id: None,
                    }],
                },
            },
            Some(&root),
        )
        .unwrap();
    let packet = receipt["result"]["id"].as_str().unwrap();
    r.complete_task(
        &p,
        &root.task_id,
        &root.thread_id,
        "安排必要的独立阅读".into(),
        None,
        10,
    )
    .unwrap();
    let work = r.work_task_for_attempt(&p, &root.task_id).unwrap();
    let digest = r.project_source_watermarks(&p).unwrap()["digest"]
        .as_str()
        .unwrap()
        .to_owned();
    r.authorize_work_budget(
        &p,
        &work.id,
        &crate::WorkBudgetAuthorization {
            expected_version: work.version,
            expected_project_revision: 0,
            expected_source_digest: digest.clone(),
            max_requests: None,
            max_tokens: None,
            deadline_at: None,
        },
        10,
    )
    .unwrap();
    let work = r.work_task(&p, &work.id).unwrap();
    let decision = r
        .prepare_work_decision(
            &p,
            &work.id,
            &crate::WorkDecisionRequest {
                expected_work_version: work.version,
                expected_revision: 0,
                expected_source_digest: digest,
            },
        )
        .unwrap()
        .step
        .expect("packet-only conversation must continue automatically");
    let mut target = decision.target;
    target["resumeStepId"] = json!(decision.id);
    let parent = r
        .create_task(CreateTaskRequest {
            task_id: "auto-decision".into(),
            project_id: p.clone(),
            thread_id: root.thread_id.clone(),
            operation: "advisory".into(),
            input_revision: 0,
            model: None,
            target_json: target.to_string(),
            input_manifest_json: json!({"authorIntent":"决定下一步"}).to_string(),
            capability_hash: "a".repeat(64),
            capability_expires_at: 9999999999,
        })
        .unwrap();
    let status = r.task_work_decision(&parent, None).unwrap();
    let action = status["readyActions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["operation"] == "reviewQuality")
        .unwrap();
    assert_eq!(action["qualityPacketId"], packet);
    let request = crate::WorkActionRequest {
        expected_work_version: status["version"].as_i64().unwrap(),
        expected_source_digest: status["sourceDigest"].as_str().unwrap().into(),
        action_key: Some(action["actionKey"].as_str().unwrap().into()),
        proposal: None,
        instruction: "SECRET_AUTHOR_DIAGNOSIS".into(),
        idempotency_key: "choose-reader".into(),
        recovery_plan: None,
    };
    let selected = r.select_work_action(&parent, &request).unwrap();
    assert_eq!(r.select_work_action(&parent, &request).unwrap(), selected);
    r.complete_task(
        &p,
        &parent.task_id,
        &parent.thread_id,
        "交给隔离读者".into(),
        None,
        10,
    )
    .unwrap();
    let settle = |r: &mut SqliteStoryRepository, id: &str| {
        r.charge_work_provider_usage(&p, &work.id, id, &json!({"runId":id,"budgetId":format!("budget-{id}"),"status":"completed","limits":{"maxRequests":null,"maxTokens":null,"maxDurationMs":null},"usage":{"requests":1,"tokens":1,"unknownTokens":0,"overrun":false}})).unwrap();
    };
    settle(&mut r, &parent.task_id);
    let mut target = selected["step"]["target"].clone();
    target["resumeStepId"] = selected["step"]["id"].clone();
    let reader = task(&mut r, &p, "reviewQuality", target, "auto-reader");
    let bound = r.work_task_for_attempt(&p, &reader.task_id).unwrap();
    assert_eq!(bound.id, work.id);
    assert_ne!(reader.thread_id, root.thread_id);
    let page = r.read_quality_packet_page(&reader, 0).unwrap();
    assert!(!page.to_string().contains("SECRET_AUTHOR_DIAGNOSIS"));
    r.submit_quality_reader(&reader, &json!({"packetHash":page["packetHash"],"summary":"动作与话语形成反差","preference":"abstain","findings":[{"label":"A","quote":"却将那把伞留在门边","effect":"透露未说出口的关心","confidence":"medium"}],"limitations":"只有一个片段"})).unwrap();
    r.complete_task(
        &p,
        &reader.task_id,
        &reader.thread_id,
        "报告已保存".into(),
        None,
        10,
    )
    .unwrap();
    settle(&mut r, &reader.task_id);
    let status = crate::work_decision_status::status(&r.connection, &p, &work.id).unwrap();
    assert!(status["receipts"]["items"].to_string().contains(packet));
    assert!(
        status["readyActions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["operation"] != "reviewQuality")
    );
    assert!(r.quality_review(&p, packet).unwrap()["report"].is_object());
    let work = r.work_task(&p, &work.id).unwrap();
    assert_ne!(work.resume_block_reason.as_deref(), Some("readerRestart"));
    let ledgers: i64 = r
        .connection
        .query_row("SELECT COUNT(*) FROM story_work_budgets_v1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(ledgers, 1, "reader must not mint another root allowance");
    let ledger = crate::work_budget_storage::load(&r.connection, &p, &work.id).unwrap();
    assert_eq!(ledger.charges.len(), 2);
    let next = r
        .prepare_work_decision(
            &p,
            &work.id,
            &crate::WorkDecisionRequest {
                expected_work_version: work.version,
                expected_revision: 0,
                expected_source_digest: r.project_source_watermarks(&p).unwrap()["digest"]
                    .as_str()
                    .unwrap()
                    .into(),
            },
        )
        .unwrap();
    assert_eq!(
        next.step.unwrap().operation,
        "advisory",
        "report returns to the original main agent"
    );
}
fn item(kind: &str) -> QualityNote {
    QualityNote {
        kind: kind.into(),
        scope: "work".into(),
        text: "用行动表达未说出的关心".into(),
        evidence: evidence(),
        question: None,
        body: None,
        relates_to: None,
        verdict: None,
    }
}
fn change(repo: &mut SqliteStoryRepository, p: &ProjectId, change: QualityAction) -> Value {
    let v = version(&repo.connection, p).unwrap();
    repo.change_quality_notebook(
        p,
        QualityMutation {
            expected_version: v,
            idempotency_key: format!("key-{v}"),
            change,
        },
        None,
    )
    .unwrap()
}
fn task(
    repo: &mut SqliteStoryRepository,
    p: &ProjectId,
    operation: &str,
    target: Value,
    id: &str,
) -> TaskRecord {
    repo.create_task(CreateTaskRequest {
        task_id: id.into(),
        project_id: p.clone(),
        thread_id: format!("thread-{id}"),
        operation: operation.into(),
        input_revision: 0,
        model: None,
        target_json: target.to_string(),
        input_manifest_json: json!({"authorIntent":"阅读","expectedCandidateRevisionId":null})
            .to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 9999999999,
    })
    .unwrap()
}
#[test]
fn author_preferences_are_reversible_without_touching_prose() {
    let (mut r, p) = fixture();
    let before = r.project_revision(&p).unwrap();
    let receipt = change(
        &mut r,
        &p,
        QualityAction::Note {
            note: item("preference"),
        },
    );
    change(
        &mut r,
        &p,
        QualityAction::Decide {
            id: receipt["result"]["id"].as_str().unwrap().into(),
            status: "withdrawn".into(),
            reason: "只适合前一场".into(),
        },
    );
    let state = r.quality_notebook(&p, None, None).unwrap();
    assert_eq!(state["notes"][0]["status"], "withdrawn");
    assert_eq!(state["notes"][0]["decisions"][0]["reason"], "只适合前一场");
    assert_eq!(before, r.project_revision(&p).unwrap());
}

#[test]
fn historical_edit_evidence_does_not_invalidate_a_current_preference_anchor() {
    let (mut r, p) = fixture();
    r.connection.execute("INSERT INTO story_revisions_v2(revision_id,project_id,object_id,object_kind,content_digest,content_json) VALUES('before-edit',?1,'draft-chapter','draft','old-digest',?2)",params![p.as_str(),json!({"body":"她很关心他。"}).to_string()]).unwrap();
    let mut preference = item("preference");
    preference.evidence.push(QualityEvidence {
        revision_id: "before-edit".into(),
        quote: "她很关心他。".into(),
        role: "reference".into(),
    });
    let saved = change(&mut r, &p, QualityAction::Note { note: preference });
    assert_eq!(
        r.quality_notebook(&p, None, None).unwrap()["notes"][0]["sourceCurrent"],
        true
    );
    change(
        &mut r,
        &p,
        QualityAction::Decide {
            id: saved["result"]["id"].as_str().unwrap().into(),
            status: "confirmed".into(),
            reason: "明确确认改稿后的取向".into(),
        },
    );
    r.connection.execute("UPDATE story_object_heads_v2 SET candidate_revision_id='before-edit' WHERE project_id=?1",[p.as_str()]).unwrap();
    assert_eq!(
        r.quality_notebook(&p, None, None).unwrap()["notes"][0]["sourceCurrent"],
        false
    );
}
#[test]
fn model_cannot_confirm_preferences_and_identical_retry_is_idempotent() {
    let (mut r, p) = fixture();
    let t = task(
        &mut r,
        &p,
        "advisory",
        json!({"type":"project"}),
        "discussion",
    );
    let request = QualityMutation {
        expected_version: 0,
        idempotency_key: "model-note".into(),
        change: QualityAction::Note {
            note: item("preference"),
        },
    };
    let receipt = r
        .change_quality_notebook(&p, request.clone(), Some(&t))
        .unwrap();
    assert_eq!(
        receipt,
        r.change_quality_notebook(&p, request, Some(&t)).unwrap()
    );
    assert_eq!(
        r.quality_notebook(&p, None, None).unwrap()["notes"][0]["status"],
        "proposed"
    );
    assert!(
        r.change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 1,
                idempotency_key: "confirm".into(),
                change: QualityAction::Decide {
                    id: receipt["result"]["id"].as_str().unwrap().into(),
                    status: "confirmed".into(),
                    reason: "model likes it".into()
                }
            },
            Some(&t)
        )
        .is_err()
    );
}
#[test]
fn rejects_forged_cross_project_quotes_stale_cas_and_marks_changed_sources() {
    let (mut r, p) = fixture();
    let mut n = item("observation");
    n.evidence[0].quote = "虚构内容".into();
    assert!(
        r.change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 0,
                idempotency_key: "bad".into(),
                change: QualityAction::Note { note: n }
            },
            None
        )
        .is_err()
    );
    change(
        &mut r,
        &p,
        QualityAction::Note {
            note: item("observation"),
        },
    );
    r.connection
        .execute(
            "UPDATE story_object_heads_v2 SET candidate_revision_id=NULL WHERE project_id=?1",
            [p.as_str()],
        )
        .unwrap();
    assert_eq!(
        r.quality_notebook(&p, None, None).unwrap()["notes"][0]["sourceCurrent"],
        false
    );
    assert!(
        r.change_quality_notebook(
            &p,
            QualityMutation {
                expected_version: 0,
                idempotency_key: "stale".into(),
                change: QualityAction::Note {
                    note: item("preference")
                }
            },
            None
        )
        .is_err()
    );
    assert!(
        source(
            &r.connection,
            &ProjectId::parse("foreign").unwrap(),
            "draft-a"
        )
        .is_err()
    );
}
#[test]
fn fidelity_assessment_preserves_goal_and_strength_evidence() {
    let (mut r, p) = fixture();
    for kind in ["goal", "strength"] {
        let receipt = change(&mut r, &p, QualityAction::Note { note: item(kind) });
        let mut outcome = item("observation");
        outcome.relates_to = Some(receipt["result"]["id"].as_str().unwrap().into());
        outcome.verdict = Some("supported".into());
        change(&mut r, &p, QualityAction::Note { note: outcome });
    }
    assert_eq!(
        r.quality_notebook(&p, None, None).unwrap()["notes"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
#[test]
fn trials_compare_real_text_and_author_feedback_does_not_adopt() {
    let (mut r, p) = fixture();
    let mut ids = Vec::new();
    for body in ["她合上门，伞却留在外面。", "门已关上。他在门边找到了伞。"]
    {
        let mut n = item("trial");
        n.question = Some("留伞如何呈现".into());
        n.body = Some(body.into());
        ids.push(
            change(&mut r, &p, QualityAction::Note { note: n })["result"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    let receipt = change(
        &mut r,
        &p,
        QualityAction::Packet {
            kind: "comparison".into(),
            sources: ids
                .into_iter()
                .map(|id| QualityPacketSource {
                    revision_id: None,
                    quote: None,
                    note_id: Some(id),
                })
                .collect(),
        },
    );
    let id = receipt["result"]["id"].as_str().unwrap();
    let packet = r.quality_packet(&p, id).unwrap();
    assert!(packet["items"][0].get("noteId").is_none());
    assert!(packet.get("question").is_none());
    change(
        &mut r,
        &p,
        QualityAction::Feedback {
            id: id.into(),
            preference: "tie".into(),
            reason: "各有合适的叙述距离".into(),
        },
    );
    let feedback = r.quality_review(&p, id).unwrap();
    assert_eq!(
        feedback["latestAuthorFeedback"][0]["reviewerKind"],
        "author"
    );
    assert_eq!(feedback["qualityGain"], "unvalidated");
    assert_eq!(r.project_revision(&p).unwrap(), 0);
}
#[test]
fn isolated_reader_requires_fresh_thread_contiguous_pages_and_real_quotes() {
    let (mut r, p) = fixture();
    let receipt = change(
        &mut r,
        &p,
        QualityAction::Packet {
            kind: "reading".into(),
            sources: vec![QualityPacketSource {
                revision_id: Some("draft-a".into()),
                quote: None,
                note_id: None,
            }],
        },
    );
    let t = task(
        &mut r,
        &p,
        "reviewQuality",
        json!({"type":"project","qualityPacketId":receipt["result"]["id"]}),
        "reader",
    );
    assert!(r.quality_notebook(&p, None, Some(&t)).is_err());
    assert!(complete_reader(&r.connection, &t).is_err());
    assert!(r.read_quality_packet_page(&t, 1).is_err());
    let page = r.read_quality_packet_page(&t, 0).unwrap();
    assert!(!page.to_string().contains("draft-a"));
    let mut report = json!({"packetHash":page["packetHash"],"summary":"嘴上拒绝和行动形成反差","preference":"abstain","limitations":"只有局部文本，属于模型假设","findings":[{"label":"A","quote":"却将那把伞留在门边","effect":"可能读出关心","confidence":"medium"}]});
    report["findings"][0]["quote"] = json!("不存在");
    assert!(r.submit_quality_reader(&t, &report).is_err());
    report["findings"][0]["quote"] = json!("却将那把伞留在门边");
    let saved = r.submit_quality_reader(&t, &report).unwrap();
    assert_eq!(saved, r.submit_quality_reader(&t, &report).unwrap());
    complete_reader(&r.connection, &t).unwrap();
    assert_eq!(
        r.quality_review(&p, receipt["result"]["id"].as_str().unwrap())
            .unwrap()["report"]["isolation"],
        "freshTaskPacketOnly"
    );
}

#[test]
fn notebook_paginates_packets_and_notes_and_keeps_task_preferences_local() {
    let (mut r, p) = fixture();
    let t = task(&mut r, &p, "advisory", json!({"type":"project"}), "local");
    let mut local = item("preference");
    local.scope = "task".into();
    r.change_quality_notebook(
        &p,
        QualityMutation {
            expected_version: 0,
            idempotency_key: "local".into(),
            change: QualityAction::Note { note: local },
        },
        Some(&t),
    )
    .unwrap();
    for _ in 0..22 {
        change(
            &mut r,
            &p,
            QualityAction::Packet {
                kind: "reading".into(),
                sources: vec![QualityPacketSource {
                    revision_id: Some("draft-a".into()),
                    quote: None,
                    note_id: None,
                }],
            },
        );
    }
    let first = r.quality_notebook(&p, None, None).unwrap();
    assert_eq!(first["packets"].as_array().unwrap().len(), 20);
    let second = r
        .quality_notebook(&p, first["nextCursor"].as_i64(), None)
        .unwrap();
    assert_eq!(second["packets"].as_array().unwrap().len(), 2);
    assert_eq!(second["notes"].as_array().unwrap().len(), 1);
    assert!(second["nextCursor"].is_null());
    let other = task(&mut r, &p, "advisory", json!({"type":"project"}), "other");
    assert_eq!(
        r.quality_notebook(&p, Some(2), Some(&other)).unwrap()["notes"],
        json!([])
    );
    assert_eq!(
        r.quality_notebook(&p, Some(2), Some(&t)).unwrap()["notes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn reader_enforces_fresh_thread_full_delivery_cancellation_and_feedback_disagreement() {
    let (mut r, p) = fixture();
    let body = format!("{}末尾唯一证据", "雨".repeat(2100));
    r.connection
        .execute(
            "UPDATE story_revisions_v2 SET content_json=?1 WHERE revision_id='draft-a'",
            [json!({"body":body}).to_string()],
        )
        .unwrap();
    let receipt = change(
        &mut r,
        &p,
        QualityAction::Packet {
            kind: "reading".into(),
            sources: vec![QualityPacketSource {
                revision_id: Some("draft-a".into()),
                quote: None,
                note_id: None,
            }],
        },
    );
    let packet = receipt["result"]["id"].as_str().unwrap();
    task(&mut r, &p, "advisory", json!({"type":"project"}), "old");
    let request = CreateTaskRequest {
        task_id: "reader".into(),
        project_id: p.clone(),
        thread_id: "thread-old".into(),
        operation: "reviewQuality".into(),
        input_revision: 0,
        model: None,
        target_json: json!({"type":"project","qualityPacketId":packet}).to_string(),
        input_manifest_json: json!({"authorIntent":"阅读"}).to_string(),
        capability_hash: "a".repeat(64),
        capability_expires_at: 9999999999,
    };
    assert!(r.create_task(request.clone()).is_err());
    let t = r
        .create_task(CreateTaskRequest {
            thread_id: "fresh".into(),
            ..request
        })
        .unwrap();
    assert!(r.read_quality_packet_page(&t, 1).is_err());
    let first = r.read_quality_packet_page(&t, 0).unwrap();
    assert_eq!(first["totalPages"], 3);
    let mut report = json!({"packetHash":first["packetHash"],"summary":"尾部阅读","preference":"A","limitations":"局部模型假设","findings":[{"label":"A","quote":"末尾唯一证据","effect":"期待后文","confidence":"low"}]});
    assert!(r.submit_quality_reader(&t, &report).is_err());
    assert!(r.read_quality_packet_page(&t, 2).is_err());
    assert_eq!(first, r.read_quality_packet_page(&t, 0).unwrap());
    r.read_quality_packet_page(&t, 1).unwrap();
    r.read_quality_packet_page(&t, 2).unwrap();
    report["packetHash"] = json!("wrong");
    assert!(r.submit_quality_reader(&t, &report).is_err());
    report["packetHash"] = first["packetHash"].clone();
    r.submit_quality_reader(&t, &report).unwrap();
    report["summary"] = json!("不能覆盖");
    assert!(r.submit_quality_reader(&t, &report).is_err());
    change(
        &mut r,
        &p,
        QualityAction::Feedback {
            id: packet.into(),
            preference: "tie".into(),
            reason: "作者与模型看法不同".into(),
        },
    );
    assert_eq!(
        r.quality_review(&p, packet).unwrap()["calibration"]["preferenceDisagreement"],
        true
    );
    r.connection
        .execute(
            "UPDATE story_tasks_v2 SET status='cancelled' WHERE task_id='reader'",
            [],
        )
        .unwrap();
    assert!(r.read_quality_packet_page(&t, 0).is_err());
    assert!(r.submit_quality_reader(&t, &report).is_err());
    assert!(complete_reader(&r.connection, &t).is_err());
}
