use std::io::{Read, Write};
use std::net::TcpListener;

use super::RuntimeClient;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

#[test]
fn malformed_associations_and_anchor_fields_do_not_dispatch_and_null_correction_is_exact() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let make_client = || RuntimeClient {
        tool_binding: Default::default(), request_dispatched: Default::default(),
        publication_gate: super::publication_gate::PublicationGate::default(),
        read_progress: Default::default(),
        completion_receipt: Default::default(),
        argument_validation: Default::default(),
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        candidate_token: "fixture-candidate".into(),
        capability: "fixture-capability".into(),
        operation: "extractCommitments".into(),
    };
    let args = json!({"projectId":"project","taskId":"extract","threadId":"root","expectedRevision":0,
        "chapterId":"chapter","expectedCandidateRevisionId":null,"idempotencyKey":"save",
        "payload":{"chapterId":"chapter","sourceRevisionId":"source","examinedRanges":[{"start":0,"end":2}],
            "candidates":[{"id":"lamp","kind":"foreshadowing","summary":"线索","relatedEntityIds":[],
                "plotThreadId":null,"characterArcId":null,"setupEvidenceRefs":[{
                    "objectId":"draft-chapter","revisionId":"source","quote":"灯亮","start":0,"end":2}],
                "fulfillmentCriteria":"灯再次亮起","targetWindow":{"kind":"open"},"status":"open",
                "fulfillmentEvidenceRefs":[],"decisionReason":null}]}});
    let meta = json!({"rootThreadId":"root","callerThreadId":"root","sessionId":"root",
        "turnId":"turn","callId":"extract-save"});
    for (field, message) in [
        (
            "plotThreadId",
            "payload.candidates[0].plotThreadId: use null or a non-empty ID",
        ),
        (
            "characterArcId",
            "payload.candidates[0].characterArcId: use null or a non-empty ID",
        ),
        (
            "sceneId",
            "payload.candidates[0].setupEvidenceRefs[0]: unexpected field; only objectId, revisionId, quote, start, end are allowed",
        ),
    ] {
        let mut malformed = args.clone();
        if field == "sceneId" {
            malformed["payload"]["candidates"][0]["setupEvidenceRefs"][0][field] = Value::Null;
        } else {
            malformed["payload"]["candidates"][0][field] = json!("");
        }
        let client = make_client();
        // Specific local validator guidance remains tested without relying on
        // preflight order; the wire contract returns the unified safe envelope.
        assert_eq!(
            super::extraction_schema::validate(&malformed["payload"]),
            Err(message.to_owned())
        );
        let error = client
            .call("submit_commitment_extraction", &malformed, &meta)
            .unwrap_err();
        let diagnostic: Value = serde_json::from_str(&error).unwrap();
        assert_eq!(diagnostic["code"], "invalid_tool_arguments");
        assert_eq!(diagnostic["tool"], "submit_commitment_extraction");
        assert_eq!(diagnostic["attempt"], 1);
        assert!(error.len() <= 1600);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    let client = make_client();
    listener.set_nonblocking(false).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut bytes = [0; 4096];
            let count = stream.read(&mut bytes).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&bytes[..count]);
            if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = std::str::from_utf8(&request[..end]).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + length {
                    let body: Value = serde_json::from_slice(&request[end + 4..]).unwrap();
                    let headers = headers.to_owned();
                    stream.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{\"candidateCount\":1}").unwrap();
                    return (headers, body);
                }
            }
        }
    });
    assert_eq!(
        client.call("submit_commitment_extraction", &args, &meta),
        Ok((201, json!({"candidateCount":1})))
    );
    let (headers, body) = server.join().unwrap();
    assert!(headers.starts_with(
        "POST /api/projects/project/chapters/chapter/commitment-extractions HTTP/1.1"
    ));
    let mut expected = args;
    expected.as_object_mut().unwrap().remove("projectId");
    expected.as_object_mut().unwrap().remove("chapterId");
    assert_eq!(body, expected);
}
