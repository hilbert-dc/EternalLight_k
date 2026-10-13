use super::{RuntimeClient, operation_digest, percent_encode, validate_execution_identity};
use serde_json::{Value, json};

pub(super) fn is_read(tool: &str) -> bool {
    let reads:Vec<String> = serde_json::from_str(include_str!("../../../shared/novel-recovery-read-tools.json")).expect("current recovery effects resource");
    reads.iter().any(|read|read==tool)
}

impl RuntimeClient {
    pub(super) fn recovery_proof(&self, name: &str, args: &Value, meta: &Value, result: &Result<(u16, Value), String>) -> Option<Value> {
        if is_read(name) || !self.operation_tools().iter().any(|t|t["name"]==name) { return None; }
        let thread = args["threadId"].as_str()?;
        validate_execution_identity(meta,thread,name).ok()?;
        if meta["callerThreadId"]!=meta["rootThreadId"] || meta["parentThreadId"].as_str().is_some_and(|s|!s.is_empty()) {return None;}
        for value in [&args["taskId"],&args["projectId"],&meta["turnId"],&meta["callId"]] {
            if value.as_str().is_none_or(|s|s.is_empty()||s.len()>160||s.chars().any(char::is_control)){return None;}
        }
        let hash=operation_digest::transport_hash(name,meta,&operation_digest::operation_digest(name,args).ok()?);
        let mut proof=json!({"authority":"novelWorkspaceMcp","projectId":args["projectId"],"runId":args["taskId"],
            "rootThreadId":meta["rootThreadId"],"callerThreadId":meta["callerThreadId"],"parentThreadId":meta["parentThreadId"],
            "turnId":meta["turnId"],"callId":meta["callId"],"toolName":name,"requestHash":hash});
        if !self.request_dispatched.get() {
            let feedback:Value=serde_json::from_str(result.as_ref().err()?).ok()?;
            if feedback["tool"]!=name || !matches!(feedback["code"].as_str(),Some("invalid_tool_arguments"|"tool_argument_correction_limit")){return None;}
            proof["phase"]="beforeDispatch".into();
            proof["outcome"]="rejected".into();
            proof["operationId"]=Value::Null;
            proof["receiptRef"]=format!("mcp:preflight:{}:{hash}",meta["callId"].as_str()?).into();
            return Some(proof);
        }
        let fields=[("taskId",args["taskId"].as_str()?), ("threadId",thread), ("turnId",meta["turnId"].as_str()?),
            ("callId",meta["callId"].as_str()?), ("toolName",name), ("requestHash",hash.as_str())];
        let query=fields.iter().map(|(key,value)|format!("{key}={}",percent_encode(value))).collect::<Vec<_>>().join("&");
        let path=format!("/api/projects/{}/tasks/recovery-operations?{query}",percent_encode(args["projectId"].as_str()?));
        let (status,response)=self.request("GET",&path,None,meta,name,&hash).ok()?;
        if status!=200 {return None;}
        let op=&response["operation"];
        for (field,expected) in [("run_id",&args["taskId"]),("thread_id",&meta["rootThreadId"]),("turn_id",&meta["turnId"]),("tool_call_id",&meta["callId"])] {
            if &op[field]!=expected {return None;}
        }
        if op["tool_name"]!=name || op["request_hash"]!=hash || op["operation_id"].as_str().is_none_or(str::is_empty) {return None;}
        proof["phase"]="runtimeLedger".into();
        proof["operationId"]=op["operation_id"].clone();
        match op["state"].as_str()? {
            "committed" => {
                let receipt=op["receipt_ref"].as_str().filter(|s|s.starts_with("runtime:")&&!s.is_empty())?;
                proof["outcome"]="committed".into();proof["receiptRef"]=receipt.into();
            }
            "rejected" => {proof["outcome"]="rejected".into();proof["receiptRef"]=format!("runtime:operation:{}:rejected",op["operation_id"].as_str()?).into();}
            "accepted"|"outcome-unknown" => {proof["outcome"]="unknown".into();proof["receiptRef"]=Value::Null;}
            _ => return None,
        }
        Some(proof)
    }
}

// Business fixture servers explicitly answer read-only ledger requests with
// absence, which remains unknown. They never fabricate committed receipts.
#[cfg(test)]
pub(super) fn accept_business_request(listener:&std::net::TcpListener) -> (std::net::TcpStream,std::net::SocketAddr) {
    use std::io::{Read,Write};
    loop {
        let (mut stream,address)=listener.accept().unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let mut bytes=[0u8;8192];
        let started=std::time::Instant::now();
        let first_line=loop {
            let count=stream.peek(&mut bytes).unwrap();
            assert!(count>0);
            if let Some(end)=bytes[..count].windows(2).position(|w|w==b"\r\n") {break String::from_utf8(bytes[..end].to_vec()).unwrap();}
            assert!(started.elapsed()<std::time::Duration::from_secs(5));
            std::thread::yield_now();
        };
        if !first_line.starts_with("GET ") || !first_line.contains("/tasks/recovery-operations?") {return (stream,address);}
        let mut header=Vec::new();
        while !header.windows(4).any(|w|w==b"\r\n\r\n") {
            let count=stream.read(&mut bytes).unwrap();assert!(count>0);header.extend_from_slice(&bytes[..count]);
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 18\r\nConnection: close\r\n\r\n{\"operation\":null}").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read,Write},net::TcpListener};

    fn client(port:u16)->RuntimeClient {
        RuntimeClient {host:"127.0.0.1".into(),port,candidate_token:"fixture".into(),capability:"capability".into(),
            operation:"generateChapter".into(),tool_binding: Default::default(), request_dispatched:Default::default(),publication_gate:Default::default(),
            read_progress:Default::default(),completion_receipt:Default::default(),argument_validation:Default::default()}
    }
    fn arguments()->Value {json!({"projectId":"project","taskId":"task","threadId":"root","expectedRevision":0,
        "expectedCandidateRevisionId":null,"idempotencyKey":"key","payload":{"chapterId":"chapter","chapterNumber":1,"title":"章","body":"文","completeness":"chapterComplete"}})}
    fn identity()->Value {json!({"sessionId":"root","rootThreadId":"root","callerThreadId":"root","parentThreadId":null,"turnId":"turn","callId":"call"})}

    #[test]
    fn before_dispatch_proof_requires_exact_local_feedback_and_trusted_root() {
        let client=client(1);let args=arguments();let meta=identity();
        let error=Err(json!({"code":"invalid_tool_arguments","tool":"save_draft_checkpoint"}).to_string());
        let proof=client.recovery_proof("save_draft_checkpoint",&args,&meta,&error).unwrap();
        assert_eq!(proof["phase"],"beforeDispatch");assert_eq!(proof["outcome"],"rejected");
        assert_eq!(proof["operationId"],Value::Null);
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&meta,&Err("HTTP request failed".into())).is_none());
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&meta,&Err(json!({"code":"invalid_tool_arguments","tool":"save_brief"}).to_string())).is_none());
        let mut child=meta.clone();child["callerThreadId"]="child".into();child["parentThreadId"]="root".into();
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&child,&error).is_none());
        let mut missing=meta;missing.as_object_mut().unwrap().remove("turnId");
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&missing,&error).is_none());
        client.request_dispatched.set(true);
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&identity(),&error).is_none(),"Transport attempt cannot be a preflight rejection");
    }

    #[test]
    fn dispatched_proof_only_uses_exact_readonly_ledger_and_preserves_accepted_unknown() {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let client=client(listener.local_addr().unwrap().port());
        let args=arguments();let hash=operation_digest::operation_digest("save_draft_checkpoint",&args).unwrap();
        let op=json!({"run_id":"task","thread_id":"root","turn_id":"turn","tool_call_id":"call","tool_name":"save_draft_checkpoint",
            "request_hash":hash,"operation_id":"operation","state":"committed","receipt_ref":"runtime:draft:revision"});
        let mut wrong=op.clone();wrong["request_hash"]="f".repeat(64).into();
        let mut accepted=op.clone();accepted["state"]="accepted".into();accepted["receipt_ref"]=Value::Null;
        let server=std::thread::spawn(move||{
            for operation in [op,wrong,accepted] {
                let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut bytes=Vec::new();while !bytes.windows(4).any(|w|w==b"\r\n\r\n") {let mut buffer=[0u8;8192];let count=stream.read(&mut buffer).unwrap();assert!(count>0);bytes.extend_from_slice(&buffer[..count]);}
                let request=String::from_utf8(bytes).unwrap();assert!(request.starts_with("GET /api/projects/project/tasks/recovery-operations?"));
                for field in ["taskId=task","threadId=root","turnId=turn","callId=call","toolName=save_draft_checkpoint","requestHash="] {assert!(request.contains(field),"{field}");}
                assert!(request.to_lowercase().contains("x-novel-caller-thread: root"));
                let body=json!({"operation":operation}).to_string();let response=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);stream.write_all(response.as_bytes()).unwrap();
            }
        });
        client.request_dispatched.set(true);
        let result=Err("Transport failed after dispatch".into());
        let committed=client.recovery_proof("save_draft_checkpoint",&args,&identity(),&result).unwrap();
        assert_eq!(committed["phase"],"runtimeLedger");assert_eq!(committed["outcome"],"committed");
        assert!(client.recovery_proof("save_draft_checkpoint",&args,&identity(),&result).is_none());
        let accepted=client.recovery_proof("save_draft_checkpoint",&args,&identity(),&result).unwrap();
        assert_eq!(accepted["outcome"],"unknown");assert_eq!(accepted["receiptRef"],Value::Null);
        server.join().unwrap();
    }
}
