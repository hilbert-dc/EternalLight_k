import { PersistentMediaLedger, MediaOperationExecutor, createOpenAIImagesAdapter, createOpenAIAudioAdapter, createRunwayAdapter, digest } from "@eternallight/agent-engine";

const factories={"openai-images":createOpenAIImagesAdapter,"openai-audio":createOpenAIAudioAdapter,runway:createRunwayAdapter};
const connectionDefinition=connection=>({id:connection.id,protocol:connection.protocol,baseUrl:connection.baseUrl,model:connection.model,credentialRealmId:required(connection.credentialRealmId,"服务凭据作用域")});
const required=(value,label)=>{if(typeof value!=="string"||!value||value.length>200)throw Error(`${label}无效`);return value;};

/** A Host boundary, not a scheduler. Domain callbacks are trusted and Runtime
 * owns every document/asset revision; existing Host lifecycle calls step/reconcile. */
export class MediaServiceHost {
  constructor({databasePath,connections,registerIntent,authorizeOperation,loadAsset,importOutput,sealOutputs,lookupOutputs,admitResource,clock=Date.now,fetchImpl,allowLocalhost=false}) {
    for(const [name,callback] of Object.entries({registerIntent,authorizeOperation,loadAsset,importOutput,sealOutputs,lookupOutputs,admitResource}))if(typeof callback!=="function")throw Error(`MediaServiceHost requires ${name}`);
    Object.assign(this,{connections,registerIntent,authorizeOperation,loadAsset,importOutput,sealOutputs,lookupOutputs,admitResource,clock,fetchImpl,allowLocalhost});
    this.ledger=new PersistentMediaLedger(databasePath,{clock});this.executors=new Map();
  }
  async adapter(connection,scope) {
    const factory=factories[connection.protocol];if(!factory)throw Error("媒体服务协议不受支持");
    return factory({...connection,fetchImpl:this.fetchImpl,allowLocalhost:this.allowLocalhost,loadAsset:id=>this.loadAsset(scope,id)});
  }
  async prepare({task,grant,request,reservation,source,target,editionId,operationId}) {
    if(!request||!task||task.projectId!==grant?.projectId)throw Error("媒体请求与任务作品不一致");
    required(task.id,"任务编号");required(editionId,"作品版本");required(request.idempotencyKey,"幂等键");required(request.providerId,"服务编号");
    if(task.workTaskId!==undefined&&task.workTaskId!==grant.workTaskId)throw Error("媒体请求与工作任务不一致");
    const connection=await this.connections.resolve(request.providerId,request.kind);
    const id=operationId??`media-${digest({projectId:grant.projectId,workTaskId:grant.workTaskId,idempotencyKey:request.idempotencyKey})}`;
    required(id,"操作编号");
    const operation={id,projectId:grant.projectId,scopeId:grant.scopeId,branchId:grant.branchId,workTaskId:grant.workTaskId,
      attemptId:task.id,editionId,providerId:request.providerId,kind:request.kind,idempotencyKey:request.idempotencyKey,
      request:request.parameters,requestDigest:digest(request.parameters),reservation,source:source??null,target:target??request.targetSelection??null,
      connection:connectionDefinition(connection)};
    await this.authorizeOperation(operation,{phase:"prepare",task,grant});
    const adapter=await this.adapter(connection,operation);await adapter.validate(operation.request,operation.kind);
    if(!adapter.inspect().capabilities.some(item=>item.kind===operation.kind&&item.available))throw Error("媒体能力尚未配置");
    // Persist the domain pin before engine reservation. An orphaned pin is safe:
    // recovery repeats the same operationId; no provider request happened yet.
    await this.registerIntent({operation,grant,task,source:operation.source,target:operation.target});
    return this.ledger.prepare(grant,operation);
  }
  async executor(record) {
    let adapters=[];
    if(!(record.state==="prepared"&&record.cancelRequested)) {
      const connection=await this.connections.resolve(record.providerId,record.kind);
      if(digest(connectionDefinition(connection))!==digest(record.connection))throw Error("原媒体服务配置已变化，请恢复原配置以核对该作业");
      adapters=[await this.adapter(connection,record)];
    }
    return new MediaOperationExecutor({ledger:this.ledger,adapters,clock:this.clock,
      authorize:(operation,context)=>this.authorizeOperation(operation,context),admitResource:(operation,context)=>this.admitResource(operation,context),lookupOutputs:operation=>this.lookupOutputs(operation),
      collectOutputs:async(operation,outputs)=>{
        if(!Array.isArray(outputs)||!outputs.length||outputs.length>16)throw Error("媒体生成结果无效");
        const assetRevisionIds=[];let artifactRevisionId=null;
        for(let index=0;index<outputs.length;index++){
          const output=outputs[index];
          const receipt=await this.importOutput(operation,{...output,outputIndex:index,outputCount:outputs.length});
          if(output.kind==="transcript")artifactRevisionId=required(receipt.artifactRevisionId,"转写成果回执");
          else assetRevisionIds.push(required(receipt.assetRevisionId,"素材回执"));
        }
        const receipt=await this.sealOutputs(operation,{assetRevisionIds,artifactRevisionId});
        if(!receipt||typeof receipt!=="object")throw Error("成果集合尚未持久确认");
        return {assetRevisionIds,receipt};
      }});
  }
  async step(operationId,{signal}={}) {
    required(operationId,"操作编号");
    if(this.executors.has(operationId))throw Error("媒体作业已在执行");
    const record=this.ledger.get(operationId);if(["ready","failed","cancelled"].includes(record.state))return record;
    // Reserve this JS slot before resolving credentials; the native OS lock and
    // fence also exclude a second Host/process owner.
    this.executors.set(operationId,null);
    try{const executor=await this.executor(record);this.executors.set(operationId,executor);return await executor.step(operationId,{signal});}finally{this.executors.delete(operationId);}
  }
  async cancel(operationId) {
    required(operationId,"操作编号");
    await this.authorizeOperation(this.ledger.get(operationId),{phase:"cancel"});
    const executor=this.executors.get(operationId);
    return executor?executor.cancel(operationId):this.ledger.requestCancel(operationId);
  }
  async reconcile({projectId,limit=20,signal}={}) {
    if(!Number.isInteger(limit)||limit<1||limit>100)throw Error("核对数量无效");
    const results=[];
    for(const record of this.ledger.list({projectId,unfinishedOnly:true}).slice(0,limit)) {
      signal?.throwIfAborted();if(this.executors.has(record.id))continue;
      try{results.push({id:record.id,record:await this.step(record.id,{signal})});}
      catch{results.push({id:record.id,record:this.ledger.get(record.id),errorCode:"MEDIA_RECONCILIATION_UNAVAILABLE"});}
    }
    return results;
  }
}
