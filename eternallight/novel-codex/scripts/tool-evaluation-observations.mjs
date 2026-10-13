// Evaluation projections only: Native owns saved content and task state.
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { safeFailure } from './tool-evaluation-report.mjs';
const digest = value => value == null ? null : createHash('sha256').update(JSON.stringify(value)).digest('hex');
const integer = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
const range = value => value && typeof value === 'object'
  ? Object.fromEntries(['start','end','total','fromEntry','toEntry','totalEntries'].filter(k=>integer(value[k])!==null).map(k=>[k,value[k]])) : null;
export function observeSourceRead(args, result) {
  return {
    source: digest(args?.sourceId), cursor: digest(args?.cursor),
    view: ['content','raw','index','workspace','locate'].includes(args?.view) ? args.view : args?.view === undefined ? 'content' : 'unknown',
    query: digest({quote:args?.quote,groupId:args?.groupId,question:args?.question,chapterNumber:args?.chapterNumber,objectKind:args?.objectKind}),
    returnedSource: digest(result?.sourceId), contentHash: digest(result?.contentHash),
    coverage: range(result?.coverage), byteRange: range(result?.byteRange), nextCursor: digest(result?.nextCursor),
    entries: Array.isArray(result?.entries) ? result.entries.map(entry=>({path:digest(entry.path),range:range(entry.characterRange),revision:digest(entry.editAnchor?.sourceRevisionId)})) : [],
    resultDigest: digest(result),
  };
}
export function observeSavedRevision(result, args) {
  return {submittedContentDigest:digest(args?.payload??args?.patch),expectedCandidate:digest(args?.expectedCandidateRevisionId),patchSource:digest(args?.patch?.sourceRevisionId),saved:result?.saved===true,revision:digest(result?.revisionId),checkpoint:digest(result?.checkpointId),version:integer(result?.version),source:result?.savedSource?.status==='available'&&result.savedSource.revisionId===result.revisionId?digest(result.savedSource.sourceId):null};
}
export function observeContext(result) {
  return { originalSource: digest(result?.taskSources?.entries?.find(entry=>entry.sectionPath==='/relevantText')?.sourceId) };
}
export function summarizeReads(tools) {
  const seen=new Set();let repeatedExactResults=0;
  for(const tool of tools) {
    if(!tool.read || tool.error)continue;
    const key=JSON.stringify(tool.read);
    if(seen.has(key))repeatedExactResults++;else seen.add(key);
  }
  return {successfulReads:tools.filter(t=>t.read&&!t.error).length,repeatedExactResults,
    interpretation:'Identical request and result observations; repetition alone does not establish waste.'};
}
export async function observeFinalState({runtime,base,taskId,original,quiescent}) {
  const observation={quiescent,contentPreserved:null,readback:{status:'notRun'},domainTaskStatus:null};
  if(base&&original) {
    try {
      const workspace=await runtime.request('GET',`${base}/creative-workspace`);
      const chapter=workspace.chapters.find(c=>c.candidate?.chapterId===original.chapterId);
      const candidate=chapter?.candidate;
      const groups=candidate?.sceneGroups;
      const expectedBody='雨'+original.sceneGroups[0].body.slice(1);
      const withoutBody=items=>items?.map(({body,...rest})=>rest);
      const checks={body:groups?.[0]?.body===expectedBody,title:candidate?.title===original.title,
        chapterIdentity:candidate?.chapterId===original.chapterId&&chapter?.number===original.chapterNumber,
        groupStructure:isDeepStrictEqual(withoutBody(groups),withoutBody(original.sceneGroups)),
        protectedGroups:isDeepStrictEqual(groups?.slice(1),original.sceneGroups.slice(1))};
      const matches=Object.values(checks).every(Boolean);
      observation.readback={status:matches?'matched':'mismatched',checks,revision:digest(candidate?.id),
        expectedBodyHash:digest(expectedBody),actualBodyHash:digest(groups?.[0]?.body),
        originalBodyHash:digest(original.sceneGroups[0].body),bodyUnchanged:groups?.[0]?.body===original.sceneGroups[0].body,
        expectedCharacters:Array.from(expectedBody).length,
        actualCharacters:typeof groups?.[0]?.body==='string'?Array.from(groups[0].body).length:null};
      observation.contentPreserved=quiescent?matches:null;
    }catch(error){observation.readback={status:'unavailable',error:safeFailure(error,'readback')};}
  }
  if(base&&taskId) {
    try {
      const task=await runtime.request('GET',`${base}/tasks/${taskId}`);
      observation.domainTaskStatus=['queued','running','waiting','completed','failed','cancelled'].includes(task.status)?task.status:'unknown';
    }catch(error){observation.taskReadbackError=safeFailure(error,'taskReadback');}
  }
  return observation;
}
