import "./planning-workspace.css";

type PlanningRevision = { id: string; version: number | null };

export interface PlanningWorkspaceHeaderProps {
  view: "brief" | "outline" | "plans";
  title: string;
  description: string;
  artifact?: {
    accepted?: PlanningRevision | null;
    candidate?: PlanningRevision | null;
    working?: PlanningRevision | null;
  };
  dirty: boolean;
  blocked: boolean;
  locked: boolean;
  hasReview: boolean;
  loading?: boolean;
  prepareDirtyDiscussion?: boolean;
  onDiscuss?: (prompt: string) => void;
  onSave: () => void;
  onReview: () => void;
  onOpenVersions?: () => void;
}

const planningNames = { brief: "创作简报", outline: "全书大纲", plans: "近章规划" };

export function PlanningWorkspaceHeader({ view, title, description, artifact, dirty, blocked, locked, hasReview, loading = false, prepareDirtyDiscussion = false, onDiscuss, onSave, onReview, onOpenVersions }: PlanningWorkspaceHeaderProps) {
  const saved = artifact?.candidate ?? artifact?.accepted;
  const working = artifact?.working;
  const name = planningNames[view];
  const alreadyAdopted = !dirty && Boolean(saved && saved.id === artifact?.accepted?.id);
  const state = loading ? "正在读取规划…" : dirty ? "有未保存修改" : !saved ? "尚未开始" : saved.id === artifact?.accepted?.id ? "已采用" : "已保存，待采用";
  const discussionDisabled = blocked || loading || dirty && !prepareDirtyDiscussion;
  const discussionPrompt = saved
    ? `请讨论当前已保存的${name}可以怎样调整，先核对来源并说明建议；先不要改写或采用。`
    : `请和我一起起草${name}。已有明确目标时据此规划，缺少关键目标时先问我；不要自动采用。`;
  const checkPrompt = `请只读检查当前已保存的${name}，说明有依据的问题、可保留的安排和未覆盖的范围。不要修改或采用规划。`;

  return <header className="planning-workspace-header" aria-label={`${title}状态与操作`}>
    <div className="planning-workspace-description"><h2 className="sr-only">{title}</h2><p>{description}</p></div>
    <div className="planning-workspace-state" role="status" aria-live="polite">
      <strong>{state}</strong>{locked && <span>已锁定</span>}
    </div>
    {view === "brief" && !loading && working && working.id !== saved?.id && <p className="planning-working-notice">后续写作仍参考另一份已保存规划。{onOpenVersions && <button type="button" className="text-button" onClick={onOpenVersions}>查看依据</button>}</p>}
    <div className="planning-workspace-actions">
      {onDiscuss && <div className="planning-discussion-actions">
        <button className="secondary-button" type="button" disabled={discussionDisabled} onClick={() => onDiscuss(discussionPrompt)}>{saved || dirty ? "讨论调整" : "开始规划"}</button>
        <button className="text-button" type="button" disabled={discussionDisabled || dirty || !saved} onClick={() => onDiscuss(checkPrompt)}>只读检查</button>
      </div>}
      <div className="planning-save-actions">
        <button className="secondary-button" type="button" disabled={blocked || loading || locked} onClick={onSave}>保存</button>
        <button className="primary-button" type="button" disabled={blocked || loading || locked || dirty || !artifact?.candidate || alreadyAdopted} onClick={onReview}>{alreadyAdopted ? "已采用" : hasReview ? "查看采用检查" : "检查并采用"}</button>
      </div>
    </div>
    {dirty && <p className="planning-action-hint">{prepareDirtyDiscussion ? "发送时先保存规划；保存成功后再讨论。" : "请先保存修改，再按已保存内容讨论或检查。"}</p>}
    {!loading && !saved && !dirty && <p className="planning-action-hint">可以先手动填写，也可以在对话中一起规划；不必先完善简报才能写作。</p>}
  </header>;
}
