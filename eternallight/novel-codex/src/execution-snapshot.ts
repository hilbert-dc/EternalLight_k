import {ExecutionEventJournal, type ExecutionEvent} from '../shared/execution-event-journal.mjs';
import type {NovelCodexApi} from './types';
export type ExecutionSnapshot = Awaited<ReturnType<NovelCodexApi['agent']['listExecutions']>>[number];

export class ExecutionSnapshotBuffer {
  private journals = new Map<string, ExecutionEventJournal>();
  capture(value:unknown, projectId:string) {
    const event=value as ExecutionEvent;
    const domain=event?.hostExecution;
    if (!domain || domain.projectId!==projectId || !Number.isSafeInteger(domain.sequence)) return true;
    if (event.method==='agent/execution-ended') {this.journals.delete(domain.id);return true;}
    let journal=this.journals.get(domain.id);
    if (!journal) {
      // At most four active Host domains; retain four extra in-flight responses.
      if(this.journals.size>=8)this.journals.delete(this.journals.keys().next().value!);
      journal=new ExecutionEventJournal();this.journals.set(domain.id,journal);
    }
    if(domain.sequence<=journal.cursor)return false;
    journal.append(event);return true;
  }
  merge(snapshot:ExecutionSnapshot) {
    const later=this.journals.get(snapshot.executionId)?.after(snapshot.cursor)??[];
    const questions=new Map(snapshot.questions.map(question=>[String(question.requestId),question]));
    for(const event of later) {
      if(event.method==='agent/user-input-requested') {
        const request=event.params?.request;
        if(request?.executionId===snapshot.executionId)questions.set(String(request.requestId),request);
      }
      if(event.method==='serverRequest/resolved')questions.delete(String(event.params?.requestId));
    }
    let journal=this.journals.get(snapshot.executionId);
    if(!journal){
      if(this.journals.size>=8)this.journals.delete(this.journals.keys().next().value!);
      journal=new ExecutionEventJournal();this.journals.set(snapshot.executionId,journal);
    }
    journal.cursor=Math.max(journal.cursor,snapshot.cursor);
    return {events:[...snapshot.events,...later],questions:[...questions.values()]};
  }
}
export const truncatedExecutionNotice='运行进度较长，当前仅恢复最近记录；完整内容请查看对话历史。';
