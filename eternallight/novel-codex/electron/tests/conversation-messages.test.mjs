import test from "node:test";
import assert from "node:assert/strict";
import { ConversationMessages } from "../conversation-messages.mjs";
const message = (extra = {}) => ({ id: "m1", projectId: "p", threadId: "t", revision: 0, status: "queued", sessionId: "host", input: { text: "解释设定", focus: null, connectionId: null }, ...extra });
test("a reopened Host never implicitly dispatches another session's queued input", async () => {
  let calls = 0;
  const transport = new ConversationMessages({ sessionId: "new-host", request: async () => { calls++; }, available: () => true, execute: async () => { calls++; } });
  assert.equal(await transport.dispatch(message(), {}), false);
  assert.equal(calls, 0);
});
test("delivery failure is recorded against the original claim without another execute", async () => {
  const changes = []; let executions = 0;
  const transport = new ConversationMessages({ sessionId: "host", available: () => true,
    request: async (path, options) => {
      const body = JSON.parse(options.body); changes.push(body.action);
      return message({ revision: body.expectedRevision + 1, status: "claimed", error: body.error });
    }, execute: async () => { executions++; throw Error("transport lost"); } });
  await assert.rejects(transport.dispatch(message(), {}), /transport lost/);
  assert.deepEqual(changes, ["claim", "fail"]); assert.equal(executions, 1);
});
test("receipt ownership mismatch blocks execute", async () => {
  let executions = 0;
  const transport = new ConversationMessages({ sessionId: "host", available: () => true,
    request: async () => message({ projectId: "foreign", status: "claimed", revision: 1 }), execute: async () => { executions++; } });
  await assert.rejects(transport.dispatch(message(), {}), /归属/); assert.equal(executions, 0);
});
test("author pause holds a queued item without marking it failed", async () => {
  const transport = new ConversationMessages({ sessionId: "host", available: () => true,
    request: async () => { throw Object.assign(Error("held"), { code: "message_held" }); }, execute: async () => assert.fail("held queue executed") });
  assert.equal(await transport.dispatch(message(), {}), false);
});
test("message binds to the task from Native atomic creation, not a guessed ID", async () => {
  const transport = new ConversationMessages({ sessionId: "host", request: async () => message({ status: "claimed", revision: 2, taskId: "task" }) });
  const delivery = { message: message({ status: "claimed", revision: 1 }) };
  await transport.bind(delivery, { id: "task" }); assert.equal(delivery.message.taskId, "task");
  await assert.rejects(transport.bind({ message: message() }, { id: "foreign" }), /绑定/);
});

test("read-only receipt recovery includes delivered messages and rejects another owner", async () => {
  const calls = [];
  const transport = new ConversationMessages({request: async (path, options) => {
    calls.push({path, options}); return message({status: "delivered"});
  }});
  assert.equal((await transport.read("p", "t", "m1")).status, "delivered");
  assert.deepEqual(calls, [{path: "/api/projects/p/conversations/t/messages/m1", options: undefined}]);
  await assert.rejects(transport.read("p", "other", "m1"), /归属/);
});

test('continuous input in the first conversation does not starve another queue',async()=>{
 const transport=new ConversationMessages({request:async()=>({conversations:[{id:'a'},{id:'b'}]}),execute:async()=>{},available:()=>true});
 transport.list=async(project,thread)=>[{id:thread,status:'queued',trigger:{kind:'immediate'}}];
 const dispatched=[];transport.dispatch=async(message,_event,started)=>{dispatched.push(message.id);started();return true;};
 for(let i=0;i<4;i++)await transport.drain('p',{});
 assert.deepEqual(dispatched,['a','b','a','b']);
});


test("reconciliation in the same Host remains held until explicit continue", async () => {
  const held = message({requiresContinue:true});
  const transport = new ConversationMessages({sessionId:"host", request:async()=>({messages:[held]}),
    available:()=>assert.fail("confirmation must precede availability"),execute:()=>assert.fail("unconfirmed replay")});
  const [listed]=await transport.list("p","t");
  assert.equal(listed.requiresContinue,true);
  assert.equal(await transport.dispatch(listed,{}),false);
});

test("media image-only submission preserves immutable references and normalizes scope", async () => {
  let accepted;
  const transport=new ConversationMessages({sessionId:"host", request:async(path,options)=>{
    if(!options)return {id:"t",projectId:"p",scopeId:"scope",isArchived:false};
    accepted=JSON.parse(options.body);return message({input:accepted.input});
  }});
  const media={scope:{projectId:"p",editionId:"edition",scopeId:"scope",branchId:"branch",name:"UI metadata"},expectedRevisionId:"revision",selection:null,imageAssetRevisionIds:["image"]};
  await transport.submit("p","t",{messageId:"m1",input:{text:"",media},trigger:{kind:"immediate"}});
  assert.equal(accepted.input.text,"");
  assert.deepEqual(accepted.input.media,{...media,scope:{projectId:"p",editionId:"edition",scopeId:"scope",branchId:"branch"}});
});


test("pending steer receipt is visible without granting another dispatch",async()=>{
  const receipt=message({id:"author-message-original",sessionId:"author-intent",status:"sending",trigger:{kind:"steer",taskId:"task"},requiresContinue:false});
  const transport=new ConversationMessages({sessionId:"new-host",request:async()=>({messages:[receipt]}),available:()=>assert.fail("steer was queued"),execute:()=>assert.fail("steer replayed")});
  const [listed]=await transport.list("p","t");
  assert.equal(listed.requiresContinue,false);assert.equal(await transport.dispatch(listed,{}),false);
});

const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function parallelQueue({available=()=>true,maxActive=2,claimGate=null}={}) {
 const rows=new Map(['a','b','c'].map(id=>[id,message({id,threadId:id,trigger:{kind:'immediate'}})]));
 const gates=new Map(),claims=[];
 const transport=new ConversationMessages({sessionId:'host',maxActive,available,request:async(path,options)=>{
   if(!options)return {conversations:[...rows.keys()].map(id=>({id}))};
   const id=path.split('/').at(-1),row=rows.get(id),body=JSON.parse(options.body);
   if(body.action==='claim'){claims.push(id);if(claimGate)await claimGate.promise;row.status='claimed';}
   else if(body.action==='fail')row.error=body.error;
   row.revision++;return structuredClone(row);
 },execute:async(_event,delivery)=>{const gate=deferred();gates.set(delivery.message.id,gate);await gate.promise;
   if(delivery.message.id==='b')throw Error('background delivery unknown');
 }});
 transport.list=async(_project,thread)=>[structuredClone(rows.get(thread))];
 return {transport,rows,gates,claims};
}
test('a long queued turn does not block other conversations; capacity and duplicate scans hold',async()=>{
 const {transport,rows,gates,claims}=parallelQueue();
 assert.equal(await transport.drain('p',{}),true);assert.equal(gates.has('a'),true);
 assert.equal(await transport.drain('p',{}),true);assert.equal(gates.has('b'),true);
 assert.equal(transport.activeCount,2);assert.equal(await transport.drain('p',{}),false);
 assert.deepEqual(claims,['a','b']);gates.get('b').resolve();
 await new Promise(r=>setImmediate(r));
 assert.match(rows.get('b').error,/unknown/);assert.equal(transport.activeCount,1);
 assert.equal(await transport.drain('p',{}),true);assert.equal(gates.has('c'),true);
 transport.pause();assert.equal(await transport.drain('p',{}),false);
 gates.get('a').resolve();gates.get('c').resolve();await transport.settled();assert.equal(transport.activeCount,0);
});
test('shutdown during claim keeps the original receipt without starting a provider',async()=>{
 const claimGate=deferred(),{transport,gates,rows,claims}=parallelQueue({claimGate});
 const scan=transport.drain('p',{});const failed=assert.rejects(scan,/关闭/);
 await new Promise(r=>setImmediate(r));assert.deepEqual(claims,['a']);
 transport.pause();claimGate.resolve();await failed;await transport.settled();
 assert.equal(gates.size,0);assert.equal(rows.get('a').status,'claimed');assert.match(rows.get('a').error,/关闭/);
});
test('simultaneous scans cannot reserve the same conversation twice',async()=>{
 const {transport,gates,claims}=parallelQueue();
 await Promise.all([transport.drain('p',{}),transport.drain('p',{})]);
 assert.deepEqual(claims,['a','b']);assert.equal(transport.activeCount,2);
 for(const gate of gates.values())gate.resolve();await transport.settled();
});
