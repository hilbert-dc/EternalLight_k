// @ts-check
const id = value => typeof value === 'string' && value.length > 0 && value.length <= 160 && value.trim() === value && !/[\p{Cc}]/u.test(value);
const scopeKeys = ['branchId','editionId','projectId','scopeId'];
export function workspaceEditContext(value) {
  if (!value || Object.keys(value).sort().join(',') !== 'chapterScope,dirty,mediaScope,projectId'
    || typeof value.dirty !== 'boolean' || value.projectId !== null && !id(value.projectId)) throw Error('编辑上下文无效');
  const chapter = value.chapterScope;
  if (chapter !== null && (!chapter || Array.isArray(chapter) || Object.keys(chapter).sort().join(',') !== 'chapterId,projectId'
    || !id(chapter.chapterId) || chapter.projectId !== value.projectId || !id(chapter.projectId) || value.mediaScope !== null)) throw Error('章节编辑范围无效');
  const scope = value.mediaScope;
  if (scope !== null && (!scope || Array.isArray(scope) || Object.keys(scope).sort().join(',') !== scopeKeys.join(',')
    || scopeKeys.some(key=>!id(scope[key])) || scope.projectId !== value.projectId)) throw Error('编辑范围不属于当前作品');
  return Object.freeze({...value,chapterScope:chapter===null?null:Object.freeze({...chapter}),mediaScope:scope===null?null:Object.freeze({...scope})});
}

// Unsaved edits belong to Renderer. The message scope is frozen by Native;
// this scheduling guard never grants a new write capability or changes a CAS.
export function messageBlockedByEdits(context, message) {
  if (!context?.dirty || context.projectId !== message.projectId
    || message.trigger.kind === 'interlude' || message.input.focus?.readOnly === true) return false;
  if (context.chapterScope && message.input.focus?.view === 'chapters'
    && id(message.input.focus.chapterId) && !message.input.media) return false;
  const edit = context.mediaScope, target = message.input.media?.scope;
  return !(edit && target && target.projectId === context.projectId
    && id(target.editionId) && id(target.scopeId) && id(target.branchId)
    && ((target.editionId !== edit.editionId && target.scopeId !== edit.scopeId)
      || (target.editionId === edit.editionId && target.scopeId === edit.scopeId && target.branchId === edit.branchId)));
}
