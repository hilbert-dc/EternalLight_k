import { useEffect, useRef, useState } from "react";

import type { EntityAuditJobSummary, FactAuditJobSummary, KnowledgeAuditJobSummary, NovelCodexApi, RelationAuditJobSummary, SearchAuditJobSummary, SnapshotKnowledgeAuditJobSummary } from "./types";
import type { AuditJob, AuditKind, DiagnosticState } from "./diagnostic-state";

type Command = "pause" | "resume" | "cancel";

interface Props {
  api: NovelCodexApi;
  projectId: string;
  refreshToken: number;
  kind: AuditKind;
  onState: (kind: AuditKind, state: DiagnosticState) => void;
}

export function IndexAuditPanel({ api, projectId, refreshToken, kind, onState }: Props) {
  const [job, setJob] = useState<AuditJob | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [reloadToken, setReloadToken] = useState(0);
  const epochRef = useRef(0);
  const readEpochRef = useRef(0);
  const busyRef = useRef(false);
  const readerRef = useRef<{ pending: boolean; refresh: (() => void) | null }>({ pending: false, refresh: null });
  const visible = job?.projectId === projectId ? job : null;
  const title = { search: "检索索引校验", entity: "人物索引校验", relation: "关系索引校验", knowledge: "断言与认知索引校验", snapshotKnowledge: "历史快照知识校验", fact: "确认事实链校验" }[kind];

  useEffect(() => {
    onState(kind, { status: error || actionError ? "unavailable" : !loaded ? "loading" : visible?.status ?? "notStarted",
      verifiedConsistent: visible?.status === "completed" ? visible.verifiedConsistent : null });
  }, [kind, onState, error, actionError, loaded, visible?.status, visible?.verifiedConsistent]);

  useEffect(() => () => { ++epochRef.current; }, []);

  const readAudit = async (): Promise<{ job: AuditJob | null }> => {
    switch (kind) {
      case "search": return api.runtime.getSearchAudit(projectId);
      case "entity": return api.runtime.getEntityAudit(projectId);
      case "relation": return api.runtime.getRelationAudit(projectId);
      case "knowledge": return api.runtime.getKnowledgeAudit(projectId);
      case "snapshotKnowledge": return api.runtime.getSnapshotKnowledgeAudit(projectId);
      case "fact": return api.runtime.getFactAudit(projectId);
    }
  };
  const startAudit = async (): Promise<AuditJob> => {
    switch (kind) {
      case "search": return api.runtime.startSearchAudit(projectId);
      case "entity": return api.runtime.startEntityAudit(projectId);
      case "relation": return api.runtime.startRelationAudit(projectId);
      case "knowledge": return api.runtime.startKnowledgeAudit(projectId);
      case "snapshotKnowledge": return api.runtime.startSnapshotKnowledgeAudit(projectId);
      case "fact": return api.runtime.startFactAudit(projectId);
    }
  };
  const controlAudit = async (current: AuditJob, command: Command): Promise<AuditJob> => {
    switch (kind) {
      case "search": return api.runtime.controlSearchAudit(projectId, current.id, command, current.version);
      case "entity": return api.runtime.controlEntityAudit(projectId, current.id, command, current.version);
      case "relation": return api.runtime.controlRelationAudit(projectId, current.id, command, current.version);
      case "knowledge": return api.runtime.controlKnowledgeAudit(projectId, current.id, command, current.version);
      case "snapshotKnowledge": return api.runtime.controlSnapshotKnowledgeAudit(projectId, current.id, command, current.version);
      case "fact": return api.runtime.controlFactAudit(projectId, current.id, command, current.version);
    }
  };

  useEffect(() => {
    let cancelled = false;
    const epoch = ++readEpochRef.current;
    const refresh = () => {
      if (readerRef.current.pending || busyRef.current) return;
      readerRef.current.pending = true;
      void readAudit().then(({ job: next }) => {
        if (cancelled || epoch !== readEpochRef.current) return;
        if (next && next.projectId !== projectId) throw new Error("校验报告不属于当前作品");
        setJob(next);
        setError(null);
        setLoaded(true);
      }).catch((reason: unknown) => {
        if (!cancelled && epoch === readEpochRef.current) {
          setError(reason instanceof Error ? reason.message : "校验状态读取失败");
          setLoaded(true);
        }
      }).finally(() => {
        readerRef.current.pending = false;
        if (readerRef.current.refresh !== refresh) readerRef.current.refresh?.();
      });
    };
    readerRef.current.refresh = refresh;
    refresh();
    const active = visible?.status === "queued" || visible?.status === "running";
    const interval = active ? window.setInterval(refresh, 1200) : undefined;
    return () => {
      cancelled = true;
      if (readerRef.current.refresh === refresh) readerRef.current.refresh = null;
      if (interval !== undefined) window.clearInterval(interval);
    };
  }, [api, kind, projectId, refreshToken, reloadToken, visible?.status]);

  async function act(command: "start" | Command) {
    if (busyRef.current || (command !== "start" && !visible)) return;
    const epoch = epochRef.current;
    ++readEpochRef.current;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    setActionError(null);
    try {
      const next = command === "start" ? await startAudit() : await controlAudit(visible!, command);
      if (epoch === epochRef.current) {
        if (next.projectId !== projectId) throw new Error("校验报告不属于当前作品");
        setJob(next);
        setLoaded(true);
      }
    } catch (reason) {
      if (epoch !== epochRef.current) return;
      setActionError(reason instanceof Error ? reason.message : "校验操作失败");
    } finally {
      busyRef.current = false;
      if (epoch === epochRef.current) {
        setBusy(false);
        setReloadToken((token) => token + 1);
      }
    }
  }

  const progress = kind === "search" ? (visible as SearchAuditJobSummary | null)?.audit.revision_count
    : kind === "knowledge" ? (visible as KnowledgeAuditJobSummary | null)?.audit.assertions.publishedBatches
      : kind === "snapshotKnowledge" ? (visible as SnapshotKnowledgeAuditJobSummary | null)?.audit.members
      : kind === "fact" ? (visible as FactAuditJobSummary | null)?.audit.facts
      : (visible as EntityAuditJobSummary | RelationAuditJobSummary | null)?.audit.publishedBatches;
  const scope = { search: "检索索引", entity: "实体声明与名称查找表", relation: "来源绑定的关系端点索引", knowledge: "来源绑定的断言、人物认知索引和当前知识头", snapshotKnowledge: "历史快照成员的知识捕获与来源关联", fact: "确认事实的采纳、撤销和 payload 权威链" }[kind];
  const labels = {
    search: { start: "开始全量校验", pause: "暂停校验", resume: "继续校验", cancel: "取消校验" },
    entity: { start: "开始人物索引校验", pause: "暂停人物校验", resume: "继续人物校验", cancel: "取消人物校验" },
    relation: { start: "开始关系索引校验", pause: "暂停关系校验", resume: "继续关系校验", cancel: "取消关系校验" },
    knowledge: { start: "开始断言与认知索引校验", pause: "暂停断言与认知索引校验", resume: "继续断言与认知索引校验", cancel: "取消断言与认知索引校验" },
    snapshotKnowledge: { start: "开始历史快照知识校验", pause: "暂停历史快照知识校验", resume: "继续历史快照知识校验", cancel: "取消历史快照知识校验" },
    fact: { start: "开始确认事实链校验", pause: "暂停确认事实链校验", resume: "继续确认事实链校验", cancel: "取消确认事实链校验" },
  }[kind];
  const restartable = !visible || ["completed", "stale", "failed", "cancelled"].includes(visible.status);
  const running = visible && ["queued", "running"].includes(visible.status);
  const controllable = visible && ["queued", "running", "paused"].includes(visible.status);
  const statusLabel = error || actionError ? "需要处理" : !loaded ? "读取中" : !visible ? "尚未校验"
    : running ? "进行中" : visible.status === "paused" ? "已暂停" : visible.status === "stale" ? "来源已变化"
      : visible.status === "failed" ? "失败" : visible.status === "cancelled" ? "已取消"
        : visible.verifiedConsistent === true ? "一致" : visible.verifiedConsistent === false ? "发现不一致" : "未完成核验";

  return <details className="creative-alert" aria-label={title}>
    <summary>{title} · {statusLabel}</summary>
    <p>{visible ? `已核对 ${progress} / ${visible.sourceCount} 个${kind === "search" ? "修订" : kind === "snapshotKnowledge" ? "快照成员" : kind === "fact" ? "确认事实" : "建模批次"}。` : "尚未执行全量校验。"} 该检查只覆盖{scope}，不代表故事知识已全部建模。</p>
    {visible?.verifiedConsistent === false && (kind === "search"
      ? <p role="alert">发现 {(visible as SearchAuditJobSummary).audit.mismatch_count} 个修订索引不一致、{(visible as SearchAuditJobSummary).audit.orphaned_documents} 个孤立文档、{(visible as SearchAuditJobSummary).audit.orphaned_grams} 个孤立索引项。检索结果可能遗漏内容。</p>
      : kind === "entity"
        ? <p role="alert">发现 {(visible as EntityAuditJobSummary).audit.missingClaims} 个缺失声明、{(visible as EntityAuditJobSummary).audit.changedClaims} 个错误声明，以及名称索引差异。实体检索结果可能遗漏或错误。</p>
        : kind === "relation"
          ? <p role="alert">发现 {(visible as RelationAuditJobSummary).audit.missingClaims} 条缺失关系、{(visible as RelationAuditJobSummary).audit.extraClaims} 条多余关系、{(visible as RelationAuditJobSummary).audit.changedClaims} 条错误关系。关系扩展结果可能遗漏或错误。</p>
          : kind === "snapshotKnowledge"
            ? <p role="alert">历史知识捕获缺失 {(visible as SnapshotKnowledgeAuditJobSummary).audit.missingCaptures} 处、错指 {(visible as SnapshotKnowledgeAuditJobSummary).audit.invalidCaptures} 处、成员来源错误 {(visible as SnapshotKnowledgeAuditJobSummary).audit.invalidMembers} 处、孤立捕获 {(visible as SnapshotKnowledgeAuditJobSummary).audit.orphanCaptures} 处。历史知识无法安全自动回填。</p>
          : kind === "fact"
            ? <p role="alert">确认事实关联错误 {(visible as FactAuditJobSummary).audit.invalidLinks} 条、载荷错误 {(visible as FactAuditJobSummary).audit.invalidPayloads} 条。涉及 {(visible as FactAuditJobSummary).audit.affectedFacts.join("、") || "未列出"}。权威事实不自动修复。</p>
          : <p role="alert">断言索引缺失 {(visible as KnowledgeAuditJobSummary).audit.assertions.missingClaims} 条、错误 {(visible as KnowledgeAuditJobSummary).audit.assertions.changedClaims} 条；来源依赖索引缺失 {(visible as KnowledgeAuditJobSummary).audit.assertions.missingDependencies} 条、多余 {(visible as KnowledgeAuditJobSummary).audit.assertions.extraDependencies} 条；人物认知索引缺失 {(visible as KnowledgeAuditJobSummary).audit.epistemic.missingClaims} 条、错误 {(visible as KnowledgeAuditJobSummary).audit.epistemic.changedClaims} 条；当前知识头缺失 {(visible as KnowledgeAuditJobSummary).audit.heads.missingHeads} 个、错误 {(visible as KnowledgeAuditJobSummary).audit.heads.changedHeads} 个、孤立 {(visible as KnowledgeAuditJobSummary).audit.heads.extraHeads} 个、来源失效 {(visible as KnowledgeAuditJobSummary).audit.heads.invalidReadyBatches} 批、来源歧义 {(visible as KnowledgeAuditJobSummary).audit.heads.ambiguousReadyBatches} 批。知识检索结果可能遗漏或错误。</p>)}
    {kind === "knowledge" && visible?.status === "completed" && visible.verifiedConsistent === false && <p role="alert">实体索引缺失 {(visible as KnowledgeAuditJobSummary).audit.entities.missingClaims} 条、多余 {(visible as KnowledgeAuditJobSummary).audit.entities.extraClaims} 条、错误 {(visible as KnowledgeAuditJobSummary).audit.entities.changedClaims} 条；名称索引缺失 {(visible as KnowledgeAuditJobSummary).audit.entities.missingNames} 条、多余 {(visible as KnowledgeAuditJobSummary).audit.entities.extraNames} 条。</p>}
    {visible?.status === "stale" && <p>来源或索引已变化，请重新开始校验。</p>}
    {visible?.status === "failed" && <p role="alert">校验失败：{visible.error ?? "未知错误"}</p>}
    {error && <p role="alert">{error}</p>}
    {actionError && <p role="alert">{actionError}</p>}
    {error && <button type="button" className="secondary-button" disabled={busy} onClick={() => setReloadToken((token) => token + 1)}>重试读取状态</button>}
    {restartable && <button type="button" className="secondary-button" disabled={busy} onClick={() => void act("start")}>{labels.start}</button>}
    {running && <button type="button" className="secondary-button" disabled={busy} onClick={() => void act("pause")}>{labels.pause}</button>}
    {visible?.status === "paused" && <button type="button" className="secondary-button" disabled={busy} onClick={() => void act("resume")}>{labels.resume}</button>}
    {controllable && <button type="button" className="secondary-button" disabled={busy} onClick={() => void act("cancel")}>{labels.cancel}</button>}
  </details>;
}
