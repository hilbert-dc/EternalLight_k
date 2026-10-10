import { useEffect, useRef, useState } from "react";
import type { GenerationHandoffProposal, NovelCodexApi } from "./types";
import { generationPermitForChapters, MAX_GENERATION_CHAPTERS } from "../shared/generation-permit.mjs";
import { ConversationPrompt } from "./ConversationPrompt";
import "./conversation-generation-handoff.css";

export function ConversationGenerationHandoff({ api, proposal, scope, blocked, onConfirm, onCancel }: {
  api: NovelCodexApi; proposal: GenerationHandoffProposal; scope: object; blocked: boolean;
  onConfirm: () => Promise<void>; onCancel: () => void;
}) {
  const owner = useRef({ api, proposal, scope, blocked });
  owner.current = { api, proposal, scope, blocked };
  const flight = useRef<object | null>(null);
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => { flight.current = null; }, []);
  const valid = proposal.state === "proposed" && Number.isSafeInteger(proposal.targetChapters)
    && proposal.targetChapters >= 1 && proposal.targetChapters <= MAX_GENERATION_CHAPTERS && proposal.authorIntent.trim();
  const permit = valid ? generationPermitForChapters(proposal.targetChapters) : null;
  async function confirm() {
    if (!valid || blocked || flight.current) return;
    const ticket = {}; flight.current = ticket; setReading(true); setError(null);
    const current = () => flight.current === ticket && owner.current.api === api
      && owner.current.proposal === proposal && owner.current.scope === scope;
    try {
      const workspace = await api.runtime.getCreativeWorkspace(proposal.projectId);
      if (!current()) return;
      if (owner.current.blocked) throw new Error("请先保存修改并等待当前任务结束，再批准创作。");
      if (workspace.project.id !== proposal.projectId || workspace.revision !== proposal.expectedRevision
        || workspace.headSnapshotId !== proposal.baseSnapshotId || workspace.sourceDigest !== proposal.sourceDigest) {
        throw new Error("作品内容已变化，本次安排尚未启动。请在对话中根据当前内容重新安排。");
      }
      await onConfirm();
    } catch (reason) {
      if (current()) setError(reason instanceof Error ? reason.message : "创作安排暂未启动，请查看任务状态。");
    } finally {
      if (current()) { flight.current = null; setReading(false); }
    }
  }
  return <section className="conversation-generation-handoff" aria-label="新章节创作安排">
    <strong>生成 {proposal.targetChapters} 章工作稿</strong>
    <ConversationPrompt text={proposal.authorIntent} detailKey={`generation:${proposal.proposalId}:goal`} />
    <p>批准后自动组织创作。{permit?.maxTaskAttempts === null && "当前未设用量上限，"}可随时暂停或停止。模型用量按实际计费，工作稿不会自动采用或发布。</p>
    {error && <p role="alert">{error}</p>}
    <div><button type="button" className="text-button" disabled={reading} onClick={onCancel}>暂不开始</button>
      <button type="button" className="secondary-button" disabled={!valid || blocked || reading} onClick={() => void confirm()}>
        {reading ? "正在核对来源…" : `批准生成 ${proposal.targetChapters} 章`}
      </button></div>
  </section>;
}
