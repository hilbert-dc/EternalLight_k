// @ts-check
import { mediaMessageInput } from "./media-conversation.mjs";
import { validateConversationId } from './conversation-handlers.mjs';
import { validateAuthorIntent } from '../shared/author-input-budget.mjs';
import { validateConversationFocus } from './conversation-analysis.mjs';
const path = (project, work) => `/api/projects/${encodeURIComponent(validateConversationId(project))}/work-tasks/${encodeURIComponent(validateConversationId(work))}`;
const post = (request, url, body) => request(url, {method:'POST',body:JSON.stringify(body)});
export class WorkInterludes {
  constructor({request,messages,poke}) { this.request = request;
    this.messages = messages;
    this.poke = poke; }
  async begin(project,work,input) {
    if (!input || Object.keys(input).some(key=>!['id','threadId','expectedControlRevision','parentId','autoReturn','text','focus','media'].includes(key))
      || typeof input.autoReturn!=='boolean' || !Number.isSafeInteger(input.expectedControlRevision)) throw Error('临时提问参数无效');
    validateConversationId(input.id); validateConversationId(input.threadId); validateAuthorIntent(input.text);
    const media=mediaMessageInput(input.media);
    if(media && (media.scope.projectId!==project || input.focus!=null))throw Error('媒体插问来源不匹配');
    const focus=media?null:validateConversationFocus({...input.focus,readOnly:true});
    const result=await post(this.request,`${path(project,work)}/interludes`,{
      id:input.id,threadId:input.threadId,expectedControlRevision:input.expectedControlRevision,parentId:input.parentId,autoReturn:input.autoReturn,
      message:{messageId:input.id,input:{text:input.text,focus,connectionId:this.messages.connectionId(),...(media?{media}:{})},
        trigger:{kind:'interlude',interludeId:input.id,workTaskId:work},sessionId:this.messages.sessionId},
    });
    if(result.id!==work || result.projectId!==project || result.threadId!==input.threadId) throw Error('临时提问回执归属不匹配');
    this.poke(); return result;
  }
  async return(project,work,command) {
    if (!command || Object.keys(command).some(key=>!['commandId','interludeId','threadId','expectedControlRevision','expectedRevision'].includes(key))) throw Error('返回参数无效');
    const result=await post(this.request,`${path(project,work)}/interludes/return`,{...command,automatic:false});
    if(result.id!==work || result.projectId!==project || result.threadId!==command.threadId) throw Error('返回回执归属不匹配');
    await this.afterAnswer(project,work);
    this.poke(); return result;
  }
  async afterAnswer(project,work) {
    try { return await this.returnAuthorizedParents(project,work); }
    catch(error) {
      // The answer is already delivered. A return failure must keep Native's
      // hold visible, without changing the successful answer into a failed turn.
      return {state:'held',error:error instanceof Error?error.message:'返回条件尚未确认'};
    }
  }
  async returnAuthorizedParents(project,work) {
    for(let depth=0;depth<8;depth++) {
      const current=await this.request(path(project,work)), item=current.interlude;
      if(current.id!==work || current.projectId!==project) throw Error('原任务归属不匹配');
      if(current.status!=='running' || !item?.autoReturn || item.phase!=='answered') return;
      const message=await this.messages.read(project,current.threadId,item.messageId);
      if(message.sessionId!==this.messages.sessionId || message.status!=='delivered') return;
      await post(this.request,`${path(project,work)}/interludes/return`,{
        commandId:item.id,interludeId:item.id,threadId:current.threadId,expectedControlRevision:current.controlRevision,
        expectedRevision:item.revision,automatic:true,
      });
    }
    this.poke();
  }
}
