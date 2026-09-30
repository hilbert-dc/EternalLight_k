use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use novel_codex_story_core::{BranchId, ProjectId, SqliteStoryRepository};
use novel_codex_story_runtime::router;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const AUTHOR: &str = "state-author-token-1234567890123456789";
const CANDIDATE: &str = "state-candidate-token-123456789012345";
const CAPABILITY: &str = "state-capability-12345678901234567890";
const THREAD: &str = "property-model-thread";

#[path = "support/state_property_audit_http_tests.rs"]
mod source_audit_tests;

enum Role {
    Author,
    Candidate,
}

async fn call(
    path: &Path,
    method: &str,
    uri: &str,
    payload: Value,
    role: Role,
) -> (StatusCode, Value) {
    let token = match role {
        Role::Author => AUTHOR,
        Role::Candidate => CANDIDATE,
    };
    let app = router(task_fixture::state(
        path.to_owned(),
        AUTHOR.into(),
        CANDIDATE.into(),
    ));
    let response = app
        .author_fixture_request(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .header("x-novel-task-capability", CAPABILITY)
                .header("content-type", "application/json")
                .body(if payload.is_null() {
                    Body::empty()
                } else {
                    Body::from(payload.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!(
                "{method} {uri}: {status}: {error}: {}",
                String::from_utf8_lossy(&bytes)
            )
        }),
    )
}

struct Fixture {
    path: PathBuf,
    project: String,
    draft: Value,
    batch: Value,
    task: Value,
    evidence: Value,
}

impl Fixture {
    async fn save_rules(&self, request: Value) -> Value {
        let (status, saved) = call(
            &self.path,
            "POST",
            &format!("/api/projects/{}/artifacts/propertyRules", self.project),
            request,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        saved
    }

    async fn adopt_rules(&self, saved: &Value, revision: i64, key: &str) -> Value {
        let base = format!("/api/projects/{}", self.project);
        let (status, review) = call(&self.path, "POST", &format!("{base}/reviews"), json!({
            "artifactKind":"propertyRules","revisionId":saved["revisionId"],"proposals":[],"selections":{},"idempotencyKey":format!("review-{key}")
        }), Role::Author).await;
        assert_eq!(status, StatusCode::CREATED, "{review}");
        let (status, receipt) = call(
            &self.path,
            "POST",
            &format!("{base}/reviews/{}/accept", review["id"].as_str().unwrap()),
            json!({
                "expectedRevision":revision,"idempotencyKey":format!("accept-{key}"),"selections":{}
            }),
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        receipt
    }

    async fn new(path: PathBuf) -> Self {
        let (status, created) = call(
            &path,
            "POST",
            "/api/projects",
            json!({"title":"属性状态"}),
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let project = created["project"]["id"].as_str().unwrap().to_owned();
        let body = "林夕携带钥匙和书，从白塔走向港口。😀";
        let (status, draft) = call(&path, "POST", &format!("/api/projects/{project}/drafts/checkpoints"), json!({
            "payload":{"chapterNumber":1,"title":"迁移","body":body},
            "expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"property-draft"
        }), Role::Author).await;
        assert_eq!(status, StatusCode::CREATED, "{draft}");
        let (status, batch) = call(&path, "POST", &format!("/api/projects/{project}/modeling-batches"), json!({
            "objectId":format!("draft-{}",draft["chapterId"].as_str().unwrap()),
            "sourceRevisionId":draft["revisionId"],"expectedRevision":0,"extractorVersion":"state-test"
        }), Role::Author).await;
        assert_eq!(status, StatusCode::CREATED, "{batch}");
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 600;
        let (status, task) = call(&path, "POST", &format!("/api/projects/{project}/tasks"), json!({
            "operation":"modelKnowledge","inputRevision":0,"threadId":THREAD,"target":{"batchId":batch["id"]},
            "authorIntent":"提取来源绑定的属性状态","capabilityExpiresAt":expires,
            "capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))
        }), Role::Author).await;
        assert_eq!(status, StatusCode::CREATED, "{task}");
        Self {
            path,
            project,
            draft,
            batch,
            task,
            evidence: json!({"quote":body,"start":0,"end":body.chars().count()}),
        }
    }

    fn claim(
        &self,
        predicate: &str,
        cardinality: &str,
        value_type: Value,
        value: Value,
        start: Value,
    ) -> Value {
        json!({"kind":"state","subjectId":"linxi","predicate":predicate,"value":value,
            "stateSchema":{"cardinality":cardinality,"valueType":value_type},"validFrom":start,"evidence":self.evidence})
    }

    async fn publish(&self, assertions: Vec<Value>) -> (StatusCode, Value) {
        let entities = [("linxi","character","林夕"),("tower","location","白塔"),("port","location","港口"),
            ("key","item","钥匙"),("book","item","书")].into_iter().map(|(id,kind,name)|
            json!({"id":id,"kind":kind,"displayName":name,"aliases":[],"evidence":self.evidence})).collect::<Vec<_>>();
        call(&self.path,"POST",&format!("/api/projects/{}/modeling-batches/{}/publish",self.project,self.batch["id"].as_str().unwrap()),
            json!({"taskId":self.task["id"],"threadId":THREAD,"expectedRevision":0,"entities":entities,"assertions":assertions}),Role::Candidate).await
    }

    async fn state(&self, query: &str) -> Value {
        let (status, value) = call(
            &self.path,
            "GET",
            &format!("/api/projects/{}/knowledge/state?{query}", self.project),
            Value::Null,
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{value}");
        value
    }

    async fn freeze(&self) -> String {
        let (status, saved) = call(
            &self.path,
            "POST",
            &format!("/api/projects/{}/drafts/checkpoints", self.project),
            json!({
                "payload":{"chapterNumber":2,"title":"次日","body":"次日。"},"expectedRevision":0,
                "expectedCandidateRevisionId":null,"idempotencyKey":"property-freeze"
            }),
            Role::Author,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{saved}");
        SqliteStoryRepository::open(&self.path)
            .unwrap()
            .branch_head(
                &ProjectId::parse(&self.project).unwrap(),
                &BranchId::parse("main").unwrap(),
            )
            .unwrap()
            .unwrap()
            .snapshot_id
            .as_str()
            .to_owned()
    }
}

fn rule_definition(cardinality: &str, value_type: Value, exceptions: Value) -> Value {
    json!({"predicate":"mood","subjectKind":"character","schema":{"cardinality":cardinality,"valueType":value_type},
        "description":"作者规则说明，不得向受限视角披露","exceptions":exceptions})
}

#[tokio::test]
async fn approved_rules_reproject_claims_and_restore_snapshot_authority_on_undo() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("rules.sqlite3")).await;
    let claims = ["警觉", "平静"].map(|value| {
        f.claim(
            "mood",
            "multiple",
            json!({"type":"text"}),
            json!(value),
            json!(1),
        )
    });
    assert_eq!(f.publish(claims.to_vec()).await.0, StatusCode::OK);
    let before = f.freeze().await;
    let rule = rule_definition("single", json!({"type":"text"}), json!([]));
    let first = f.save_rules(json!({"payload":{"eventRules":[],"definitions":[rule]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"rule-first"})).await;
    let candidate = f.state("perspective=world&predicate=mood").await;
    assert_eq!(candidate["propertyRulesRevisionId"], Value::Null);
    assert_eq!(candidate["findings"], json!([]));
    f.adopt_rules(&first, /*revision*/ 0, "first").await;
    let approved = f.state("perspective=world&predicate=mood").await;
    assert_eq!(
        approved["findings"][0]["schemaAuthority"],
        "authorConfirmed"
    );
    assert_eq!(approved["entries"][0]["confirmationState"], "candidate");
    assert_eq!(approved["entries"][0]["assertion"], claims[0]);
    assert_eq!(
        approved["entries"][0]["propertyRuleAssessment"]["status"],
        "declarationMismatch"
    );
    let frozen = SqliteStoryRepository::open(&f.path)
        .unwrap()
        .branch_head(
            &ProjectId::parse(&f.project).unwrap(),
            &BranchId::parse("main").unwrap(),
        )
        .unwrap()
        .unwrap()
        .snapshot_id;
    assert_eq!(
        f.state(&format!(
            "perspective=world&predicate=mood&snapshotId={before}"
        ))
        .await["propertyRulesRevisionId"],
        Value::Null
    );
    let reader = f
        .state("perspective=reader&readingChapter=1&predicate=mood")
        .await;
    assert_eq!(reader["propertyRulesRevisionId"], Value::Null);
    assert!(!reader.to_string().contains("作者规则说明"));
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    let base = format!("/api/projects/{}", f.project);
    let (status, writer) = call(&f.path,"POST",&format!("{base}/tasks"),json!({
        "operation":"generateChapter","inputRevision":1,"threadId":"rules-writer","target":{"chapterId":SqliteStoryRepository::open(&f.path).unwrap().chapters(&ProjectId::parse(&f.project).unwrap()).unwrap()[1].chapter_id,"chapterNumber":2,"readingChapter":2,"sceneOrder":1,"storyTime":2},
        "authorIntent":"续写","capabilityExpiresAt":expires,"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{writer}");
    let (status,report) = call(&f.path,"POST",&format!("{base}/tasks/{}/query",writer["id"].as_str().unwrap()),json!({
        "threadId":"rules-writer","expectedRevision":1,"plan":{"question":"前文情绪","byteBudget":9000,"ranking":{"type":"literal"},"steps":[{"id":"mood","operation":{"type":"state","entityId":"linxi","perspective":"world","predicate":"mood"}}]}
    }),Role::Candidate).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert!(!report.to_string().contains("作者规则说明"));
    assert_eq!(
        report["steps"][0]["results"][0]["stateSchemaAuthority"],
        "modelDeclared"
    );
    let second = f.save_rules(json!({"payload":{"eventRules":[],"definitions":[rule_definition("multiple",json!({"type":"text"}),json!([]))]},"expectedRevision":1,"expectedCandidateRevisionId":first["revisionId"],"idempotencyKey":"rule-second"})).await;
    let receipt = f.adopt_rules(&second, /*revision*/ 1, "second").await;
    assert_eq!(
        f.state("perspective=world&predicate=mood").await["findings"],
        json!([])
    );
    assert_eq!(
        f.state(&format!(
            "perspective=world&predicate=mood&snapshotId={frozen}"
        ))
        .await["findings"][0]["schemaAuthority"],
        "authorConfirmed"
    );
    let (status, undone) = call(
        &f.path,
        "POST",
        &format!(
            "{base}/commits/{}/undo",
            receipt["commitId"].as_str().unwrap()
        ),
        json!({"expectedRevision":2,"idempotencyKey":"undo-rule"}),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{undone}");
    assert_eq!(
        f.state("perspective=world&predicate=mood").await["propertyRulesRevisionId"],
        first["revisionId"]
    );
    let conditioned = f.save_rules(json!({"payload":{"eventRules":[],"definitions":[rule_definition("single",json!({"type":"text"}),json!(["分身允许同时具有不同情绪"]))]},"expectedRevision":3,"expectedCandidateRevisionId":second["revisionId"],"idempotencyKey":"rule-conditional"})).await;
    f.adopt_rules(&conditioned, /*revision*/ 3, "conditional")
        .await;
    let conditional = f.state("perspective=world&predicate=mood").await;
    assert_eq!(conditional["coverage"]["unappliedPropertyRules"], 2);
    assert_eq!(
        conditional["entries"][0]["propertyRuleAssessment"]["status"],
        "requiresInterpretation"
    );
    assert_eq!(
        conditional["entries"][0]["effectiveStateSchema"],
        Value::Null
    );
}

#[tokio::test]
async fn rule_validation_permissions_and_type_mismatch_never_rewrite_source_values() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("rule-errors.sqlite3")).await;
    let claim = f.claim(
        "mood",
        "single",
        json!({"type":"text"}),
        json!("警觉"),
        json!(1),
    );
    assert_eq!(f.publish(vec![claim.clone()]).await.0, StatusCode::OK);
    let base = format!("/api/projects/{}", f.project);
    let valid = rule_definition("single", json!({"type":"integer"}), json!([]));
    let request = json!({"payload":{"eventRules":[],"definitions":[valid.clone()]},"expectedRevision":0,"expectedCandidateRevisionId":null,"idempotencyKey":"test-rule"});
    assert_eq!(
        call(
            &f.path,
            "POST",
            &format!("{base}/artifacts/propertyRules"),
            request.clone(),
            Role::Candidate
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    for (index, definitions) in [
        json!([valid.clone(), valid.clone()]),
        json!([rule_definition(
            "single",
            json!({"type":"entity","kind":"unknown"}),
            json!([])
        )]),
        json!([rule_definition(
            "single",
            json!({"type":"integer","execute":"x"}),
            json!([])
        )]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bad = request.clone();
        bad["payload"]["definitions"] = definitions;
        bad["idempotencyKey"] = json!(format!("bad-rule-{index}"));
        assert_eq!(
            call(
                &f.path,
                "POST",
                &format!("{base}/artifacts/propertyRules"),
                bad,
                Role::Author
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let saved = f.save_rules(request.clone()).await;
    assert_eq!(f.save_rules(request.clone()).await, saved);
    assert_eq!(call(&f.path,"POST",&format!("{base}/reviews"),json!({"artifactKind":"propertyRules","revisionId":saved["revisionId"],"proposals":[{}],"idempotencyKey":"mixed-rule"}),Role::Author).await.0,StatusCode::FORBIDDEN);
    f.adopt_rules(&saved, /*revision*/ 0, "typed").await;
    // An uncertain save replays its original receipt, even after formal adoption.
    assert_eq!(f.save_rules(request.clone()).await, saved);
    let mut changed_request = request;
    changed_request["payload"]["definitions"][0]["description"] = json!("changed rule");
    assert_eq!(
        call(
            &f.path,
            "POST",
            &format!("{base}/artifacts/propertyRules"),
            changed_request,
            Role::Author,
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let state = f.state("perspective=world&predicate=mood").await;
    assert_eq!(
        state["entries"][0]["propertyRuleAssessment"]["status"],
        "typeMismatch"
    );
    assert_eq!(state["entries"][0]["assertion"], claim);
    assert_eq!(state["entries"][0]["effectiveStateSchema"], Value::Null);
    assert_eq!(state["coverage"]["unappliedPropertyRules"], 1);
}

#[tokio::test]
async fn typed_properties_keep_overlap_members_unknown_time_and_schema_disagreement_distinct() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("projection.sqlite3")).await;
    let entity = json!({"type":"entity","kind":"location"});
    let mut tower = f.claim(
        "location",
        "single",
        entity.clone(),
        json!({"entityId":"tower"}),
        json!(1),
    );
    tower["validUntilExclusive"] = json!(3);
    tower["validUntilEvidence"] = f.evidence.clone();
    let claims = vec![
        tower.clone(),
        f.claim(
            "location",
            "single",
            entity,
            json!({"entityId":"port"}),
            json!(2),
        ),
        f.claim(
            "carried",
            "multiple",
            json!({"type":"entity","kind":"item"}),
            json!({"entityId":"key"}),
            json!(1),
        ),
        f.claim(
            "carried",
            "multiple",
            json!({"type":"entity","kind":"item"}),
            json!({"entityId":"book"}),
            json!(1),
        ),
        f.claim(
            "age",
            "single",
            json!({"type":"integer"}),
            json!(21),
            json!(1),
        ),
        f.claim(
            "mood",
            "single",
            json!({"type":"text"}),
            json!("警觉"),
            json!(1),
        ),
        f.claim(
            "alive",
            "single",
            json!({"type":"boolean"}),
            json!(true),
            json!(1),
        ),
        f.claim(
            "undated",
            "single",
            json!({"type":"text"}),
            json!("尚未定时"),
            Value::Null,
        ),
        f.claim(
            "disputed",
            "single",
            json!({"type":"boolean"}),
            json!(true),
            json!(1),
        ),
        f.claim(
            "disputed",
            "single",
            json!({"type":"integer"}),
            json!(1),
            json!(1),
        ),
    ];
    let (status, published) = f.publish(claims).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let overlap = f.state("perspective=world&worldTime=2").await;
    assert_eq!(overlap["entries"].as_array().unwrap().len(), 9);
    let findings = overlap["findings"].as_array().unwrap();
    assert_eq!(
        findings
            .iter()
            .map(|row| row["type"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("stateValueConflict"),
            json!("stateSchemaDisagreement")
        ]
    );
    assert_eq!(findings[0]["schemaAuthority"], "modelDeclared");
    assert_eq!(findings[0]["leftEvidence"], f.evidence);
    assert_eq!(overlap["coverage"]["unknownTimeSkipped"], 1);
    let targeted = f
        .state("perspective=world&worldTime=1&entityId=tower")
        .await;
    assert_eq!(targeted["entries"].as_array().unwrap().len(), 1);
    assert_eq!(targeted["entries"][0]["assertion"], tower);
    assert_eq!(
        targeted["entries"][0]["canonicalStateValue"],
        json!({"entityId":"tower"})
    );
    assert_eq!(targeted["entries"][0]["canonicalTargetId"], "tower");
    assert_eq!(targeted["coverage"]["unknownStateTimes"], 0);
    assert_eq!(targeted["coverage"]["complete"], true);
    let later = f
        .state("perspective=world&worldTime=3&predicate=location")
        .await;
    assert_eq!(later["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        later["entries"][0]["assertion"]["value"],
        json!({"entityId":"port"})
    );
    assert_eq!(later["findings"], json!([]));
    let bag = f
        .state("perspective=world&worldTime=2&predicate=carried")
        .await;
    assert_eq!(bag["entries"].as_array().unwrap().len(), 2);
    assert_eq!(bag["findings"], json!([]));
    let unknown = f.state("perspective=world&predicate=undated").await;
    assert_eq!(unknown["coverage"]["unknownStateTimes"], 1);
    assert_eq!(unknown["coverage"]["complete"], false);
    let character = f
        .state("perspective=character&ownerId=linxi&epistemicTime=2")
        .await;
    assert_eq!(character["entries"], json!([]));
    assert_eq!(character["coverage"]["unknownStateTimes"], 0);
    let hidden = f.state("perspective=reader&readingChapter=0").await;
    assert_eq!(hidden["entries"], json!([]));
    let (status, revealed) = call(
        &f.path,
        "GET",
        &format!(
            "/api/projects/{}/knowledge/state?perspective=world",
            f.project
        ),
        Value::Null,
        Role::Candidate,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{revealed}");
}

#[tokio::test]
async fn invalid_state_publication_rolls_back_before_any_claim_or_knowledge_head() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("validation.sqlite3")).await;
    let valid = f.claim(
        "location",
        "single",
        json!({"type":"entity","kind":"location"}),
        json!({"entityId":"tower"}),
        json!(1),
    );
    let mutations = [
        ("value", Value::Null),
        ("value", json!({"entityId":"missing"})),
        ("value", json!({"entityId":"key"})),
        ("value", json!({"entityId":"tower","extra":true})),
        ("validFrom", json!("yesterday")),
        ("validUntilExclusive", json!(1)),
        ("storyTime", json!(1)),
        (
            "stateSchema",
            json!({"cardinality":"lastWins","valueType":{"type":"entity","kind":"location"}}),
        ),
        (
            "stateSchema",
            json!({"cardinality":"single","valueType":{"type":"entity","kind":"location","ddl":"drop"}}),
        ),
        (
            "evidence",
            json!({"quote":"不存在的原文","start":0,"end":7}),
        ),
    ];
    for (field, value) in mutations {
        let mut bad = valid.clone();
        bad[field] = value;
        let (status, error) = f.publish(vec![valid.clone(), bad]).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {error}");
        let repository = SqliteStoryRepository::open(&f.path).unwrap();
        let project = ProjectId::parse(&f.project).unwrap();
        let audit = repository.audit_assertion_index(&project).unwrap();
        assert_eq!((audit.published_batches, audit.expected_claims), (0, 0));
        assert!(
            repository
                .current_knowledge(&project)
                .unwrap()
                .iter()
                .all(|source| source["status"] != "ready")
        );
    }
    for (value_type, value) in [
        (json!({"type":"integer"}), json!(1.5)),
        (json!({"type":"boolean"}), json!("true")),
        (json!({"type":"text"}), json!(" ")),
        (json!({"type":"text"}), json!("海".repeat(171))),
    ] {
        let (status, error) = f
            .publish(vec![f.claim(
                "typed",
                "single",
                value_type,
                value,
                json!(1),
            )])
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    }
    let (status, published) = f.publish(vec![valid]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    assert_eq!(published["status"], "ready");
}

#[tokio::test]
async fn source_updates_do_not_rewrite_frozen_state_and_expiry_does_not_invent_an_inverse() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("history.sqlite3")).await;
    let mut claim = f.claim(
        "alive",
        "single",
        json!({"type":"boolean"}),
        json!(true),
        json!(1),
    );
    claim["validUntilExclusive"] = json!(3);
    claim["validUntilEvidence"] = f.evidence.clone();
    let (status, published) = f.publish(vec![claim.clone()]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let snapshot = f.freeze().await;
    let query = format!("perspective=world&worldTime=2&predicate=alive&snapshotId={snapshot}");
    let before = f.state(&query).await;
    let (status, trace) = call(
        &f.path,
        "GET",
        &format!(
            "/api/projects/{}/knowledge/evolution?dimension=world&snapshotId={snapshot}",
            f.project
        ),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{trace}");
    assert_eq!(trace["entries"].as_array().unwrap().len(), 2);
    assert_eq!(trace["entries"][1]["change"], "stateExpired");
    assert_eq!(trace["entries"][1]["before"], true);
    assert_eq!(trace["entries"][1]["after"], Value::Null);
    assert_eq!(
        trace["entries"][1]["interpretation"],
        "claimNoLongerApplies"
    );
    assert_eq!(trace["entries"][1]["evidence"], f.evidence);
    let (status,revised)=call(&f.path,"POST",&format!("/api/projects/{}/drafts/checkpoints",f.project),json!({
        "payload":{"chapterId":f.draft["chapterId"],"chapterNumber":1,"title":"改稿","body":"林夕离开了。"},
        "expectedRevision":0,"expectedCandidateRevisionId":f.draft["revisionId"],"idempotencyKey":"property-revised"
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{revised}");
    let current = f
        .state("perspective=world&worldTime=2&predicate=alive")
        .await;
    assert_eq!(current["entries"], json!([]));
    assert_eq!(current["coverage"]["complete"], false);
    assert_eq!(f.state(&query).await, before);
    let (_, frozen) = call(
        &f.path,
        "GET",
        &format!(
            "/api/projects/{}/knowledge/evolution?dimension=world&snapshotId={snapshot}",
            f.project
        ),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(frozen, trace);
}

#[tokio::test]
async fn entity_value_lookup_is_rebuildable_and_does_not_grant_pov_world_knowledge() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("index.sqlite3")).await;
    let claim = f.claim(
        "carried",
        "multiple",
        json!({"type":"entity","kind":"item"}),
        json!({"entityId":"key"}),
        json!(1),
    );
    let (status, published) = f.publish(vec![claim]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let project = ProjectId::parse(&f.project).unwrap();
    let mut repository = SqliteStoryRepository::open(&f.path).unwrap();
    let audit = repository.audit_assertion_index(&project).unwrap();
    assert!(audit.complete());
    assert_eq!(audit.expected_claims, 1);
    assert_eq!(repository.rebuild_assertion_index(&project).unwrap(), audit);
    let (status, next_draft) = call(
        &f.path,
        "POST",
        &format!("/api/projects/{}/drafts/checkpoints", f.project),
        json!({
            "payload":{"chapterNumber":2,"title":"续写","body":"次日。"},"expectedRevision":0,
            "expectedCandidateRevisionId":null,"idempotencyKey":"property-writer-draft"
        }),
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{next_draft}");
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + 600;
    for pov in [Value::Null, json!("linxi")] {
        let mut target = json!({"chapterId":next_draft["chapterId"],"chapterNumber":2,"readingChapter":2,"sceneOrder":1,"storyTime":2});
        if !pov.is_null() {
            target["povCharacterId"] = pov.clone();
        }
        let (status,writer)=call(&f.path,"POST",&format!("/api/projects/{}/tasks",f.project),json!({
            "operation":"generateChapter","inputRevision":0,"threadId":"property-writer","target":target,
            "authorIntent":"续写","capabilityExpiresAt":expires,"capabilityHash":format!("{:x}",Sha256::digest(CAPABILITY.as_bytes()))
        }),Role::Author).await;
        assert_eq!(status, StatusCode::CREATED, "{writer}");
        let (status,result)=call(&f.path,"GET",&format!("/api/projects/{}/tasks/{}/search?threadId=property-writer&expectedRevision=0&q=&entityName=%E9%92%A5%E5%8C%99&entityId=key",f.project,writer["id"].as_str().unwrap()),Value::Null,Role::Candidate).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(
            result["knowledge"]["results"].as_array().unwrap().len(),
            usize::from(pov.is_null())
        );
    }
}

#[tokio::test]
async fn confirming_a_state_preserves_its_schema_and_does_not_inject_a_timeless_context_fact() {
    let directory = tempfile::tempdir().unwrap();
    let f = Fixture::new(directory.path().join("confirmation.sqlite3")).await;
    let claim = f.claim(
        "mood",
        "single",
        json!({"type":"text"}),
        json!("警觉"),
        json!(1),
    );
    let (status, published) = f.publish(vec![claim.clone()]).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let base = format!("/api/projects/{}", f.project);
    let (status,review)=call(&f.path,"POST",&format!("{base}/reviews"),json!({
        "artifactKind":"draft","revisionId":f.draft["revisionId"],"modelingBatchId":f.batch["id"],
        "proposals":[],"selections":{},"idempotencyKey":"property-review"
    }),Role::Author).await;
    assert_eq!(status, StatusCode::CREATED, "{review}");
    let fact = &review["factProposals"][0];
    assert_eq!(fact["modelingAssertion"], claim);
    assert_eq!(
        fact["stateProperty"],
        json!({"schema":claim["stateSchema"],"validFrom":1,"validUntilExclusive":null,"schemaAuthority":"modelDeclared"})
    );
    let (status,accepted)=call(&f.path,"POST",&format!("{base}/reviews/{}/accept",review["id"].as_str().unwrap()),json!({
        "expectedRevision":0,"idempotencyKey":"property-accept",
        "selections":{"sceneGroupIds":[fact["sceneGroupId"]],"factDecisions":{fact["id"].as_str().unwrap():"confirmed"}}
    }),Role::Author).await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    let state = f
        .state("perspective=world&worldTime=1&predicate=mood")
        .await;
    assert_eq!(state["entries"][0]["confirmationState"], "confirmed");
    assert_eq!(state["entries"][0]["stateSchemaAuthority"], "modelDeclared");
    let (status, context) = call(
        &f.path,
        "GET",
        &format!("{base}/context?projectRevision=1&purpose=planner&budget=24&byteBudget=60000"),
        Value::Null,
        Role::Author,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(context["stateFacts"], json!([]));
    assert_eq!(
        context["coverage"]["scannedScope"]["typedStateFactsWithheld"],
        1
    );
    assert!(
        context["coverage"]["missingScope"]
            .as_array()
            .unwrap()
            .contains(&json!("typedStateFacts"))
    );
}
#[path = "support/author_task_fixture.rs"]
mod task_fixture;
use task_fixture::AuthorTaskFixture;
