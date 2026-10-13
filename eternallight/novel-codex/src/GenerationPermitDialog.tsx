import type { Dispatch, SetStateAction } from "react";
import { useEffect, useRef, useState } from "react";
import { authorIntentError } from "../shared/author-intent-validation.mjs";
import { generationPermitForChapters, MAX_GENERATION_CHAPTERS } from "../shared/generation-permit.mjs";
import type { GenerationPermitReview, WorkspaceView } from "./creative-workspace-types";
import { revisionPredecessorIsCurrent } from "./revision-outcome-decision";
import type { CreativeWorkspace, NovelCodexApi, RevisionPredecessor } from "./types";

type Input = {
  blocked: boolean;
  generationPermitReview: GenerationPermitReview | null;
  projectId: string;
  setGenerationPermitReview: Dispatch<SetStateAction<GenerationPermitReview | null>>;
  onDismissRevisionContinuation: (() => void) | undefined;
  generationReadError: boolean;
  setGenerationReload: Dispatch<SetStateAction<number>>;
  workspace: CreativeWorkspace | null;
  api: NovelCodexApi | undefined;
  view: WorkspaceView;
  onGenerateWork: (intent: string, targetChapters: number, expectedProjectRevision: number, expectedBaseSnapshotId: string, expectedSourceDigest: string, revisionPredecessor: RevisionPredecessor | null) => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
};
export function GenerationPermitDialog({ blocked, generationPermitReview, projectId, setGenerationPermitReview, onDismissRevisionContinuation, generationReadError, setGenerationReload, workspace, api, view, onGenerateWork, setError }: Input) {

  const [generationConfirming, setGenerationConfirming] = useState(false);
  const [generationPermitError, setGenerationPermitError] = useState<string | null>(null);
  const generationPermit = generationPermitForChapters(generationPermitReview?.targetChapters ?? 1);
  const generationPermitInputError = generationPermitReview ? authorIntentError(generationPermitReview.intent) : null;
  const confirmationRef = useRef({ review: generationPermitReview, api, projectId, view, workspace, blocked });
  confirmationRef.current = { review: generationPermitReview, api, projectId, view, workspace, blocked: blocked || Boolean(generationPermitInputError) };
  const confirmingRef = useRef(false);
  useEffect(() => () => { confirmationRef.current.review = null; }, []);
  useEffect(() => { setGenerationPermitError(null); }, [generationPermitReview]);
 return <>{generationPermitReview?.projectId === projectId && <div className="modal-backdrop"><section className="project-dialog generation-permit-dialog" role="dialog" aria-modal="true" aria-labelledby="generation-permit-title">
        <div className="project-dialog-header"><h2 id="generation-permit-title">确认本轮自动创作</h2><button className="dialog-close" type="button" aria-label="关闭创作范围确认" onClick={() => { setGenerationPermitReview(null); onDismissRevisionContinuation?.(); }}>×</button></div>
        {generationPermitReview.revisionPredecessor && <p className="project-dialog-description">后继创作来源：写作尝试 {generationPermitReview.revisionPredecessor.writerTaskId} · 判断 {generationPermitReview.revisionPredecessor.assessmentRevisionId} · 作者决策 {generationPermitReview.revisionPredecessor.decisionId}</p>}
        <p className="project-dialog-description">将在当前作品中生成至多 {generationPermit.targetChapters} 章可审阅工作稿，不自动采用正式版本。这里确认的是创作目标与范围，不是用量额度。</p>
        <label className="generation-permit-intent"><strong>本轮目标</strong><textarea aria-label="本轮创作目标" disabled={generationConfirming} value={generationPermitReview.intent} onChange={(event) => setGenerationPermitReview((current) => current ? { ...current, intent: event.target.value } : null)} /></label>
        {generationPermitInputError && <p className="creative-alert error" role="alert">{generationPermitInputError}</p>}
        <fieldset className="generation-scope-choice" disabled={generationConfirming}><legend>本轮创作范围</legend><label>新增章节数<input aria-label="新增章节数" type="number" min={1} max={MAX_GENERATION_CHAPTERS} step={1} value={generationPermitReview.targetChapters} onChange={(event) => { const count = Number(event.target.value); if (Number.isSafeInteger(count) && count >= 1 && count <= MAX_GENERATION_CHAPTERS) setGenerationPermitReview((current) => current ? { ...current, targetChapters: count } : null); }} /></label></fieldset>
        <p>执行至本次创作范围完成，可随时暂停或停止。模型用量按供应商实际计费。</p>
        <p className="project-dialog-description">暂停、重试与重规划会保留已保存的候选及用量记录。供应商限流、连接故障或授权失效仍可能中断执行；不会因为固定累计额度停止。</p>
        {generationReadError && <p className="creative-alert error" role="alert">创作任务状态未能核对，暂不能开始新一轮。<button className="text-button" type="button" onClick={() => setGenerationReload(value => value + 1)}>核对任务状态</button></p>}
        {(workspace?.revision !== generationPermitReview.expectedProjectRevision
          || workspace?.headSnapshotId !== generationPermitReview.expectedBaseSnapshotId
          || workspace?.sourceDigest !== generationPermitReview.expectedSourceDigest)
          && <p className="creative-alert error" role="alert">作品来源已变化，请返回编辑并重新核对创作目标。</p>}
        {generationPermitError && <p className="creative-alert error" role="alert">{generationPermitError}</p>}
        <div className="project-dialog-actions"><button className="secondary-button" type="button" onClick={() => { setGenerationPermitReview(null); onDismissRevisionContinuation?.(); }}>返回编辑</button><button className="primary-button" type="button" disabled={generationConfirming || blocked || Boolean(generationPermitInputError)
          || workspace?.revision !== generationPermitReview.expectedProjectRevision
          || workspace?.headSnapshotId !== generationPermitReview.expectedBaseSnapshotId
          || workspace?.sourceDigest !== generationPermitReview.expectedSourceDigest} onClick={() => {
            if (confirmingRef.current) return;
            const review = generationPermitReview;
            confirmingRef.current = true; setGenerationConfirming(true); setGenerationPermitError(null);
            void (async () => {
              let started = false;
              try {
                if (review.revisionPredecessor) {
                  if (!api) throw new Error("修改结果读取接口不可用");
                  const receipt = await api.runtime.getRevisionOutcome(projectId, review.revisionPredecessor.writerTaskId);
                  if (!revisionPredecessorIsCurrent(receipt, review.revisionPredecessor)) throw new Error("修改结果或最新作者决策已变化，请返回报告重新准备后继创作。");
                }
                const current = confirmationRef.current;
                if (current.review !== review || current.api !== api || current.projectId !== projectId || current.view !== view) return;
                if (current.blocked) throw new Error("请先保存修改并核对创作任务状态，再确认本轮创作。");
                if (current.workspace?.revision !== review.expectedProjectRevision || current.workspace.headSnapshotId !== review.expectedBaseSnapshotId
                  || current.workspace.sourceDigest !== review.expectedSourceDigest) throw new Error("作品来源已变化，请重新核对创作目标。");
                setGenerationPermitReview(null); onDismissRevisionContinuation?.();
                started = true;
                await onGenerateWork(review.intent.trim(), review.targetChapters, review.expectedProjectRevision,
                  review.expectedBaseSnapshotId, review.expectedSourceDigest, review.revisionPredecessor);
              } catch (reason) {
                if (confirmationRef.current.review === review) setGenerationPermitError(reason instanceof Error ? reason.message : "自动创作未能启动");
                else if (started && confirmationRef.current.projectId === projectId && confirmationRef.current.view === view) setError(reason instanceof Error ? reason.message : "自动创作未能启动");
              } finally { confirmingRef.current = false; setGenerationConfirming(false); }
            })();
          }}>{generationConfirming ? "正在核对来源…" : "确认开始创作"}</button></div>
      </section></div>}</>;
}
