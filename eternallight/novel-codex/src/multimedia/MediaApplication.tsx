import { MediaInterludePanel } from "./MediaInterludePanel";
import { isComposerSubmit } from "../composer-keyboard";
import { useMediaExecutions, type MediaSubmission } from "./useMediaExecutions";
import { useConversationOutbox } from "../useConversationOutbox";
import { ConversationMessageQueue } from "../ConversationMessageQueue";
import { useReasoningSelection } from "../useReasoningSelection";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { CreativeEdition, MediaSelection, MultimediaWorkspace } from "../../shared/multimedia-types";
import type { NovelCodexApi, ThreadDetail, ThreadSummary } from "../types";
import { MultimediaWorkspaceView } from "./MultimediaWorkspaceView";
import type { MediaEditionSource } from "./CreateMediaEditionDialog";
import { MediaServiceSettings } from "./MediaServiceSettings";
import { createAgentRun } from "../agent-events";
import { ConversationTimeline } from "../ConversationTimeline";
import { ConversationReader } from "../ConversationReader";
import { ConversationPrompt } from "../ConversationPrompt";
import { MarkdownContent } from "../MarkdownContent";
import { ConversationReplyActions } from "../ConversationReplyActions";
import { ComposerModelPicker } from "../ComposerModelPicker";
import { ModelConnectionNotice } from "../ModelConnectionNotice";
import { useModelConnection } from "../useModelConnection";
import { ConversationNavigator } from "../ConversationNavigator";
import { useSelectedConversation } from "../useSelectedConversation";
import { AuthorQuestionCard } from "../AuthorQuestionCard";
import { authorIntentError } from "../../shared/author-input-budget.mjs";
import "./media-application.css";

type AgentStatus = Awaited<ReturnType<NovelCodexApi["agent"]["status"]>>;
export function MediaApplication({ api, edition, authorKey, topbar, onBusyChange, onDirtyChange, onSaveHandlerChange, onDiscardHandlerChange, onEditionSourceChange }: {
  api: NovelCodexApi; edition: CreativeEdition; authorKey: string; topbar: ReactNode;
  onBusyChange: (busy: boolean) => void; onDirtyChange: (dirty: boolean) => void;
  onSaveHandlerChange: (handler: (() => Promise<boolean>) | null) => void;
  onDiscardHandlerChange: (handler: (() => boolean) | null) => void;
  onEditionSourceChange?: (source: MediaEditionSource | null) => void;
}) {
  const { projectId, scopeId, id: editionId } = edition;
  const selected = useSelectedConversation(JSON.stringify([authorKey, scopeId]), projectId);
  const outbox=useConversationOutbox(api,projectId,selected.id);
  const draftKey = `novel-codex:media-message:${JSON.stringify([authorKey, projectId, scopeId, selected.id])}`;
  const [draft, setDraft] = useState(() => { try { return localStorage.getItem(draftKey) ?? ""; } catch { return ""; } });
  const [heldWork,setHeldWork]=useState<{threadId:string|null;held:boolean}>({threadId:null,held:false});
  const onHoldChange=useCallback((threadId:string|null,held:boolean)=>setHeldWork({threadId,held}),[]);
  const interludeHeld=heldWork.threadId===selected.id && heldWork.held;
  const [draftError, setDraftError] = useState<string | null>(null);
  const [status, setStatus] = useState<AgentStatus | null>(null);
  const [connections, setConnections] = useState<Awaited<ReturnType<NovelCodexApi["agent"]["listConnections"]>>["connections"]>([]);
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [detail, setDetail] = useState<ThreadDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [archived, setArchived] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [settings, setSettings] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [selection, setSelection] = useState<MediaSelection | null>(null);
  const [attachments, setAttachments] = useState<{ id: string; label: string; url: string }[]>([]);
  const [verifyingImages, setVerifyingImages] = useState(false);
  const imageVerificationPending = useRef(false);
  const [attaching, setAttaching] = useState(false), [creating, setCreating] = useState(false);
  const alive = useRef(true);
  const historyEpoch = useRef(0), listEpoch = useRef(0), attachmentEpoch = useRef(0), createPending = useRef(false), attachPending = useRef(false);
  const draftRef = useRef({ key: draftKey, text: draft }); draftRef.current = { key: draftKey, text: draft };
  const selectionRef = useRef(selected); selectionRef.current = selected;
  const executions=useMediaExecutions({api,projectId,scopeId,editionId,branchId:edition.branchId,selectedId:selected.id,
    onReceipt:submission=>{acceptSubmittedDraft(submission);void outbox.refresh();},
    onSettled:()=>{setRefresh(value=>value+1);void outbox.refresh();}});
  const {runRef,pending,eventOwner,executionRequest,preparingCancelled,commitRun,setQuestions,setExecutionError}=executions.bind(executions.current);
  const run=runRef.current,busy=pending.current,sentImages=executions.current.images,questions=executions.current.questions;
  const question=questions[0]??null;
  useEffect(() => { alive.current = true; return () => { alive.current = false; historyEpoch.current++; listEpoch.current++; attachmentEpoch.current++; onBusyChange(false); }; }, []);
  const reasoningSelection = useReasoningSelection({ api, provider: status?.provider, model: status?.model,
    onChange: reasoningEffort => setStatus(current => current ? { ...current, reasoningEffort } : current) });
  const modelConnection = useModelConnection({ api, scope: `${projectId}:${scopeId}`, blocked: busy || verifyingImages, onStatus: setStatus, onConnections: setConnections });
  const selectedConnection = connections.find(item => item.provider === status?.provider && item.model === status?.model);
  const imageInputAvailable = selectedConnection?.status === "verified" && selectedConnection.verifiedCapabilities.includes("imageInput");
  const imageInputBlocked = attachments.length > 0 && !imageInputAvailable;
  useEffect(() => { onBusyChange(busy || modelConnection.busy || verifyingImages); }, [busy, modelConnection.busy, verifyingImages, onBusyChange]);
  useEffect(() => {
    const epoch = ++attachmentEpoch.current;
    try { setDraft(localStorage.getItem(draftKey) ?? ""); setDraftError(null); } catch { setDraftError("消息草稿暂时无法恢复，请保留当前输入。"); }
    setAttachments([]);
    let ids: string[] = []; try { const saved = JSON.parse(localStorage.getItem(`${draftKey}:images`) ?? "[]"); if (Array.isArray(saved)) ids = saved.filter(item => typeof item === "string"); } catch { setDraftError("图片引用尚未恢复，请重新选择素材。"); }
    if (ids.length) void api.multimedia.readWorkspace(projectId, editionId, edition.branchId).then(workspace => { if (!alive.current || attachmentEpoch.current !== epoch) return; const owned = workspace.assets.filter(asset => ids.includes(asset.assetRevisionId) && asset.kind === "image" && asset.projectId === projectId && asset.scopeId === scopeId); setAttachments(owned.map(asset => ({ id: asset.assetRevisionId, label: asset.name, url: asset.previewUrl }))); if (owned.length !== ids.length) setDraftError("部分图片引用已不可用，请重新选择后发送。"); }).catch(() => { if (alive.current && attachmentEpoch.current === epoch) setDraftError("图片引用未读取，请重新选择或稍后重试。"); });
  }, [api, draftKey, projectId, editionId, edition.branchId, scopeId]);
  const changeDraft = (text: string) => { setDraft(text); try { localStorage.setItem(draftKey, text); setDraftError(null); } catch { setDraftError("消息草稿尚未保存，请勿关闭应用。"); } };
  const readThreads = useCallback(async () => {
    const epoch = ++listEpoch.current;
    try { const result = await api.agent.listThreads(projectId, archived, null, scopeId); if (alive.current && listEpoch.current === epoch) setThreads(result.threads); }
    catch (reason) { if (alive.current && listEpoch.current === epoch) setError(reason instanceof Error ? reason.message : "对话列表未读取"); }
  }, [api, projectId, scopeId, archived]);
  useEffect(() => { void readThreads(); }, [readThreads, refresh]);
  useEffect(() => {
    let cancelled = false;
    const read = () => Promise.all([api.agent.status(), api.agent.listConnections()]).then(([value, available]) => { if (!cancelled) { setStatus(value); setConnections(available.connections); } }).catch(() => {});
    void read(); const timer = window.setInterval(read, 2000);
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [api]);
  function acceptSubmittedDraft(submission:MediaSubmission){
    try {
      if(localStorage.getItem(submission.key)===submission.text)localStorage.removeItem(submission.key);
      if(localStorage.getItem(`${submission.key}:images`)===JSON.stringify(submission.images))localStorage.removeItem(`${submission.key}:images`);
      localStorage.removeItem(`${submission.key}:submission`);
      if(draftRef.current.key===submission.key){
        setDraft(current=>current===submission.text?"":current);setDraftError(null);
        setAttachments(current=>current.filter(image=>!submission.images.includes(image.id)));
      }
    } catch {setDraftError("消息已接收，草稿存储尚未同步，请核对原消息。");}
  };
  useEffect(()=>{
    let disposed=false;
    void(async()=>{try {
      const raw=localStorage.getItem(`${draftKey}:submission`);if(!raw)return;
      const submission=JSON.parse(raw);
      if(!submission.messageId || !submission.threadId){setDraftError("上次媒体消息尚未确认，请从原对话核对交付；不会自动重发。");return;}
      const receipt=await api.agent.readMessage(projectId,submission.threadId,submission.messageId);
      if(disposed || receipt.id!==submission.messageId || receipt.input.text!==submission.text
        || receipt.input.media?.scope.scopeId!==scopeId || JSON.stringify(receipt.input.media.imageAssetRevisionIds)!==JSON.stringify(submission.images))return;
      acceptSubmittedDraft(submission);
    }catch{if(!disposed && localStorage.getItem(`${draftKey}:submission`))setDraftError("上次媒体消息的接收状态尚未核对，输入已保留。");}})();
    return()=>{disposed=true;};
  },[api,projectId,scopeId,draftKey]);
  async function readConversation(threadId: string | null) {
    const epoch = ++historyEpoch.current; setDetail(null); setError(null);
    if (!threadId) { setLoading(false); return; }
    setLoading(true);
    try { const result = await api.agent.readConversation(projectId, threadId); if (alive.current && historyEpoch.current === epoch && selectionRef.current.current.current.id === threadId) { if (result.thread.id !== threadId || result.thread.projectId && result.thread.projectId !== projectId) throw new Error("对话归属已变化，尚未显示。"); setDetail(result); const active = runRef.current; if (!pending.current && active.threadId === threadId && result.turns.some(turn => turn.id === active.turnId)) commitRun(createAgentRun()); } }
    catch (reason) { if (alive.current && historyEpoch.current === epoch) setError(reason instanceof Error ? reason.message : "对话未读取"); }
    finally { if (alive.current && historyEpoch.current === epoch) setLoading(false); }
  }
  useEffect(() => { void readConversation(selected.id); }, [selected.id, refresh]);
  async function createThread(sourceThreadId?: string, lastTurnId?: string) {
    if (createPending.current || attachPending.current) return;
    createPending.current = true; setCreating(true); const original = selected.current.current.id;
    try { const result = await api.agent.createThread(projectId, { scopeId, ...(sourceThreadId ? { sourceThreadId, lastTurnId } : {}) }); if (!alive.current) return; if (selected.current.current.id === original) { selected.select(result.thread.id); setAttachments([]); } await readThreads(); }
    catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "新对话尚未创建"); }
    finally { createPending.current = false; if (alive.current) setCreating(false); }
  }
  async function send() {
    if (interludeHeld) {setError("原任务正在临时提问，请先返回原任务。");return;}
    if (executions.current.pending.current || createPending.current || attachPending.current || modelConnection.busy || verifyingImages || dirty || !draft.trim() && !attachments.length || !status?.configured || imageInputBlocked) return;
    const inputError = authorIntentError(draft, { imageAssetRevisionIds: attachments.map(item => item.id) }); if (inputError) { setError(inputError); return; }
    const operation=executions.begin(selected.id);
    const {pending,preparingCancelled,executionRequest,submittedDraft,eventOwner,runRef,commitRun,setSentImages,setQuestions,setBusy,setExecutionError}=executions.bind(operation);
    preparingCancelled.current=false;setError(null);
    const prompt = draft, originalAttachments = attachments, originalDraftKey = draftKey, originalThreadId = selected.id;
    submittedDraft.current={key:originalDraftKey,text:prompt,images:originalAttachments.map(image=>image.id),threadId:originalThreadId,messageId:null};
    try {
      const workspace: MultimediaWorkspace = await api.multimedia.readWorkspace(projectId, editionId, edition.branchId);
      if (!alive.current || preparingCancelled.current) return;
      if (workspace.edition.id !== editionId || workspace.edition.scopeId !== scopeId || workspace.project.id !== projectId) throw new Error("作品来源已变化，尚未发送。");
      if (originalThreadId) { const previous = await api.agent.readConversation(projectId, originalThreadId); if (!alive.current || preparingCancelled.current) return; if (previous.thread.id !== originalThreadId || previous.thread.projectId && previous.thread.projectId !== projectId) throw new Error("对话来源已变化，尚未发送。"); if (selected.current.current.id === originalThreadId) setDetail(previous); }
      commitRun({ ...createAgentRun(prompt), threadId: originalThreadId }); setSentImages(originalAttachments); setQuestions([]);
      eventOwner.current = { projectId, scopeId, taskId: null, threadId: originalThreadId, requestedThreadId: originalThreadId };
      const result = await executions.invoke(operation,()=>api.agent.sendPrompt(projectId, prompt, null, null, originalThreadId,
        { scope: edition, expectedRevisionId: workspace.revisionId, selection,
          imageAssetRevisionIds: originalAttachments.map(item => item.id) }));
      if (!alive.current || preparingCancelled.current) return;
      const active = runRef.current;
      commitRun({ ...active, answer: result.message || active.answer, state: active.state === "cancelled" ? "cancelled" : "completed", completedAt: Date.now() });
      if(submittedDraft.current){acceptSubmittedDraft(submittedDraft.current);submittedDraft.current=null;}
      setRefresh(value => value + 1);
      await readThreads();
      if (alive.current && !originalThreadId && selected.current.current.id === null && active.threadId) {
        const nextKey = `novel-codex:media-message:${JSON.stringify([authorKey, projectId, scopeId, active.threadId])}`;
        try { const remaining = localStorage.getItem(originalDraftKey); if (remaining) localStorage.setItem(nextKey, remaining); localStorage.removeItem(originalDraftKey); } catch { setDraftError("新对话草稿尚未保存，请保留当前输入。"); }
        selected.select(active.threadId);
      }
    } catch (reason) {
      if (preparingCancelled.current) return;
      if (alive.current) { const message = reason instanceof Error ? reason.message : "本轮未完成"; setExecutionError(message); const active = runRef.current; commitRun({ ...active, state: active.state === "cancelled" ? "cancelled" : "error", error: message, completedAt: Date.now() }); }
    } finally { executionRequest.current = null; eventOwner.current = null; pending.current = false; if (alive.current) { setBusy(false); setQuestions([]); } }
  }
  async function stopExecution() {
    if (!pending.current) return;
    const target = executionRequest.current;
    if (!target) {
      preparingCancelled.current = true;
      setExecutionError("准备已停止，原输入和图片仍保留。");
      return;
    }
    try {
      const result = await api.agent.interrupt(target);
      if (alive.current && executionRequest.current === target && !result.interrupted) setExecutionError("停止尚未确认，请稍后重试。");
    } catch (reason) {
      if (alive.current && executionRequest.current === target) setExecutionError(reason instanceof Error ? reason.message : "停止未完成");
    }
  }
  async function attach(files?: File[]) {
    if (attachPending.current || pending.current || createPending.current) return;
    attachPending.current = true; setAttaching(true); const key = draftKey, epoch = attachmentEpoch.current;
    try { const bytes = files ? await Promise.all(files.map(async file => ({ name: file.name, bytes: new Uint8Array(await file.arrayBuffer()) }))) : undefined; const result = await api.multimedia.importFiles(projectId, editionId, bytes); if (!alive.current || attachmentEpoch.current !== epoch || draftRef.current.key !== key) return; const images = result.assets.filter(asset => asset.kind === "image" && asset.projectId === projectId && asset.scopeId === scopeId).map(asset => ({ id: asset.assetRevisionId, label: asset.name, url: asset.previewUrl })); setAttachments(items => { const next = [...new Map([...items, ...images].map(item => [item.id, item])).values()]; try { localStorage.setItem(`${key}:images`, JSON.stringify(next.map(item => item.id))); } catch { setDraftError("图片引用暂未保存，请勿关闭应用。"); } return next; }); if (result.assets.some(asset => asset.kind !== "image")) setError("素材已导入作品；当前对话支持添加图片。"); setRefresh(value => value + 1); }
    catch (reason) { if (alive.current && attachmentEpoch.current === epoch) setError(reason instanceof Error ? reason.message : "附件未导入"); }
    finally { attachPending.current = false; if (alive.current) setAttaching(false); }
  }
  async function verifyImages() {
    if (imageVerificationPending.current || busy || modelConnection.busy || selectedConnection?.status !== "verified") return;
    imageVerificationPending.current = true;
    setVerifyingImages(true); setError(null);
    try {
      await api.agent.verifyImageInput(selectedConnection.provider, selectedConnection.model, status?.reasoningEffort);
      const available = await api.agent.listConnections();
      if (!alive.current) return;
      setConnections(available.connections);
      if (!available.connections.some(item => item.provider === selectedConnection.provider && item.model === selectedConnection.model
        && item.status === "verified" && item.verifiedCapabilities.includes("imageInput"))) throw Error("当前模型尚未通过图片识别验证。图片与输入已保留。");
    } catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "图片输入尚未验证，原素材已保留。"); }
    finally { imageVerificationPending.current = false; if (alive.current) setVerifyingImages(false); }
  }
  useEffect(() => {
    if (!settings) return;
    const previous = document.activeElement as HTMLElement | null;
    document.querySelector<HTMLInputElement>(".media-settings-dialog input")?.focus();
    return () => { if (previous?.isConnected) previous.focus({ preventScroll: true }); };
  }, [settings]);
  const liveVisible = Boolean((run.prompt || sentImages.length) && (run.threadId ? run.threadId === selected.id || pending.current && selected.id === null && eventOwner.current?.requestedThreadId === null : selected.id === null));
  return <div className="app-shell media-application">{topbar}<div className="media-application-body">
    <main className="media-editor-region"><MultimediaWorkspaceView authorKey={authorKey} api={api.multimedia} projectId={projectId} editionId={editionId} busy={busy} refreshToken={refresh}
      onSourceSelectionChange={onEditionSourceChange} onSelectionChange={setSelection} onDirtyChange={value => { setDirty(value); onDirtyChange(value); }}
      onSaveHandlerChange={onSaveHandlerChange} onDiscardHandlerChange={onDiscardHandlerChange} /></main>
    <aside className="media-conversation" aria-label="创作对话"><ConversationNavigator threads={threads} selectedId={selected.id} runningIds={executions.runningIds} busy={creating || attaching} loading={loading} error={null} archived={archived}
      onSelect={thread => { if (createPending.current) return; selected.select(thread.id); setAttachments([]); }} onCreate={() => void createThread()} onRetry={() => void readThreads()} onToggleArchived={() => setArchived(value => !value)}
      onRename={async (thread, title) => { await api.agent.renameThread(projectId, thread.id, title); await readThreads(); }}
      onPin={thread => void api.agent.setThreadPinned(projectId, thread.id, !thread.isPinned).then(readThreads).catch(reason => { if (alive.current) setError(reason instanceof Error ? reason.message : "置顶未完成"); })}
      onArchive={thread => void (archived ? api.agent.unarchiveThread(projectId, thread.id) : api.agent.archiveThread(projectId, thread.id)).then(readThreads).catch(reason => { if (alive.current) setError(reason instanceof Error ? reason.message : "归档未完成"); })} />
      <div className="conversation-history-scroll media-conversation-scroll">
        <ConversationReader emptyDescription={edition.medium === "comic" ? "描述想画的故事，或选中画格一起调整。" : "描述想制作的动画，或选中镜头一起调整。"} detail={detail && run.turnId && liveVisible ? { ...detail, turns: detail.turns.filter(turn => turn.id !== run.turnId) } : detail} loading={loading} error={null} onRetry={() => void readConversation(selected.id)} onFork={turnId => void createThread(selected.id ?? undefined, turnId)} busy={busy} showEmpty={!liveVisible} />
        {liveVisible && <article className="media-live-turn">{run.prompt && <div className="conversation-message user-message"><ConversationPrompt text={run.prompt} detailKey={`${scopeId}:${run.taskId}:prompt`} /></div>}{sentImages.length > 0 && <div className="media-attachments" aria-label="本轮图片">{sentImages.map(item => <img key={item.id} src={item.url} alt={item.label} />)}</div>}<ConversationTimeline run={run} />{run.answer && <><MarkdownContent text={run.answer} /><ConversationReplyActions identity={`${scopeId}:${run.turnId}`} text={run.answer} /></>}</article>}
        {question && liveVisible && <AuthorQuestionCard requestIdentity={`${scopeId}:${question.executionId}:${question.requestId}`} questions={question.questions} onResolve={async response => { const original = question.requestId; await api.agent.resolveUserInput({ projectId, executionId: question.executionId, requestId: original }, response); if (alive.current) setQuestions(current => current.filter(item => String(item.requestId) !== String(original))); return true; }} />}
      </div>
      <ConversationMessageQueue items={outbox.items} busy={busy} edit={outbox.edit} move={outbox.move} remove={outbox.remove} reconcile={outbox.reconcile} continueWaiting={outbox.continue}/>
      {outbox.error && <p role="alert">{outbox.error}</p>}
      <div className="media-composer" onDragOver={event => { if (event.dataTransfer.types.includes("Files")) event.preventDefault(); }} onDrop={event => { event.preventDefault(); if (event.dataTransfer.files.length) void attach([...event.dataTransfer.files]); }} onPaste={event => { if (event.clipboardData.files.length) { event.preventDefault(); void attach([...event.clipboardData.files]); } }}>{(error || executions.current.error) && <p role="alert">{error || executions.current.error}</p>}{draftError && <p role="alert">{draftError}</p>}
        {selection && <p className="media-selection-context">{selection.label}<button type="button" className="icon-button" aria-label="取消选区引用" onClick={() => setSelection(null)}>×</button></p>}
        {attachments.length > 0 && <div className="media-attachments">{attachments.map(item => <span key={item.id}><img src={item.url} alt={item.label} /><button type="button" disabled={busy || attaching} aria-label={`移除${item.label}`} onClick={() => setAttachments(items => { const next = items.filter(value => value.id !== item.id); try { localStorage.setItem(`${draftKey}:images`, JSON.stringify(next.map(value => value.id))); } catch { setDraftError("图片引用尚未保存。"); } return next; })}>×</button></span>)}</div>}
        <MediaInterludePanel api={api} projectId={projectId} editionId={editionId} scopeId={scopeId} branchId={edition.branchId}
          threadId={selected.id} text={draft} selection={selection} images={attachments.map(item=>item.id)}
          blocked={attaching || creating || modelConnection.busy || imageInputBlocked || !status?.configured}
          onHoldChange={onHoldChange} acceptDraft={()=>{const submission={key:draftKey,text:draft,images:attachments.map(item=>item.id),threadId:selected.id,messageId:null};return()=>acceptSubmittedDraft(submission);}}/>
        <textarea aria-label="创作要求" aria-describedby="media-composer-keyboard-hint" placeholder={edition.medium === "comic" ? "聊聊漫画构想，或选中画格一起修改…" : "聊聊动画构想，或选中镜头一起修改…"} value={draft} onChange={event => changeDraft(event.target.value)} onKeyDown={event => { if (isComposerSubmit(event.nativeEvent,"modifierEnter")) { event.preventDefault(); void send(); } }} /><small id="media-composer-keyboard-hint">Ctrl/⌘+Enter 发送 · Shift+Enter 换行</small>
        {dirty && <p role="status">请先保存当前编辑，再发送创作要求。临时提问只读取已保存版本，当前编辑会保留。</p>}
        {imageInputBlocked && <p role="status">{connections.some(item => item.status === "verified" && item.verifiedCapabilities.includes("imageInput")) ? "当前模型不能读取图片，请选择已验证的识图模型。图片与输入会保留。" : "当前没有已验证的识图模型。图片已保留，可移除消息中的图片后继续文字创作。"}</p>}
        {imageInputBlocked && selectedConnection?.status === "verified" && <div><button type="button" className="text-button" disabled={busy || modelConnection.busy || verifyingImages} onClick={() => void verifyImages()}>{verifyingImages ? "正在验证图片输入…" : "验证当前模型的图片输入"}</button><small>将向当前服务发送两张测试图，不发送你的素材。</small></div>}
        <ModelConnectionNotice progress={modelConnection.progress} busy={modelConnection.busy} onRetry={() => void modelConnection.retry()} />
        {reasoningSelection.error && <p role="alert">{reasoningSelection.error}</p>}<div className="media-composer-actions"><button type="button" className="text-button" disabled={busy || creating || attaching} onClick={() => void attach()} aria-label="添加图片">{attaching ? "导入中…" : "＋ 图片"}</button><button type="button" className="icon-button" disabled={busy} aria-label="媒体服务设置" title="媒体服务设置" onClick={() => setSettings(true)}>⚙</button><ComposerModelPicker connections={connections} provider={status?.provider} model={status?.model} reasoningEffort={status?.reasoningEffort} label={modelConnection.busy ? "正在配置…" : status?.model ?? "选择创作模型"} disabled={busy || modelConnection.busy || verifyingImages || creating || attaching} reasoningDisabled={modelConnection.busy || verifyingImages || reasoningSelection.busy} onReasoningChange={effort => void reasoningSelection.change(effort)} onSelect={connection => void modelConnection.run(connection.provider, connection.model, connection.status === "verified" ? "apply" : "verifyAndApply", connection.reasoning?.effort ?? null)} />{busy ? <button type="button" className="icon-button" aria-label="停止本轮" onClick={() => void stopExecution()}>■</button> : <button type="button" className="primary-button" aria-label="发送创作要求" disabled={dirty || creating || attaching || modelConnection.busy || verifyingImages || imageInputBlocked || !status?.configured || !draft.trim() && !attachments.length} onClick={() => void send()}>↑</button>}</div>
      </div>
    </aside></div>{settings && <div className="modal-backdrop"><section className="project-dialog media-settings-dialog" role="dialog" aria-modal="true" aria-label="媒体服务设置" onKeyDown={event => {
      if (event.key === "Escape") { const close = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "关闭"); if (close && !close.disabled) { event.preventDefault(); close.click(); } }
      if (event.key !== "Tab") return;
      const controls = [...event.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled)')].filter(item => !item.matches(":disabled"));
      if (!controls.length) { event.preventDefault(); return; }
      const first = controls[0], last = controls.at(-1)!;
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    }}><MediaServiceSettings api={api.multimedia} onClose={() => setSettings(false)} /></section></div>}</div>;
}
