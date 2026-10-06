import { useEffect, useRef, useState } from "react";
import type { NovelCodexApi, ProjectSummary } from "./types";
import "./project-information.css";
import { QualityNotebook } from "./QualityNotebook";

const briefFields = [["premise", "核心命题"], ["promise", "读者承诺"], ["stakes", "核心代价"], ["endingDirection", "结局方向"]] as const;
type BriefRead = { owner: object; status: "ready"; source: "adopted" | "saved" | "empty"; newer: boolean; fields: { label: string; text: string }[] }
  | { owner: object; status: "error"; message: string };

export function ProjectInformation({ api, project, onClose, onOpenBrief }: {
  api?: NovelCodexApi; project: ProjectSummary; onClose: () => void; onOpenBrief: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [read, setRead] = useState<BriefRead | null>(null);
  const [retry, setRetry] = useState(0);
  const [qualityOpen, setQualityOpen] = useState(false);
  const scope = useRef({ api, projectId: project.id });
  if (scope.current.api !== api || scope.current.projectId !== project.id) scope.current = { api, projectId: project.id };
  const owner = scope.current;
  useEffect(() => {
    const element = dialog.current;
    const previous = document.activeElement as HTMLElement | null;
    element?.showModal();
    return () => {
      element?.close();
      const target = previous?.isConnected && previous !== document.body ? previous
        : document.querySelector<HTMLElement>("[data-project-information-trigger], .project-switcher:not(:disabled)");
      target?.focus({ preventScroll: true });
    };
  }, []);
  useEffect(() => {
    let current = true;
    setRead(null);
    const load = async () => {
      try {
        if (!api || !project.id) throw new Error("作品信息暂时无法读取，请确认工作区连接。");
        const workspace = await api.runtime.getCreativeWorkspace(project.id);
        if (!current || scope.current !== owner) return;
        if (workspace.project.id !== project.id) throw new Error("作品信息来源不一致，请重新读取。");
        const brief = workspace.artifacts.brief;
        const revision = brief.accepted ?? brief.candidate;
        const fields = briefFields.map(([key, label]) => {
          const value = revision?.payload[key];
          if (value !== undefined && typeof value !== "string") throw new Error("创作简报字段格式无效，请重新读取。");
          return { label, text: typeof value === "string" && value.trim() ? value : "尚未填写" };
        });
        setRead({ owner, status: "ready", source: brief.accepted ? "adopted" : revision ? "saved" : "empty",
          newer: Boolean(brief.accepted && brief.candidate && brief.accepted.id !== brief.candidate.id), fields });
      } catch (error) {
        if (current && scope.current === owner) setRead({ owner, status: "error", message: error instanceof Error ? error.message : "作品信息读取失败。" });
      }
    };
    void load();
    return () => { current = false; };
  }, [api, project.id, owner, retry]);
  const reading = read?.owner === owner ? read : null;
  return <dialog ref={dialog} className="project-information" aria-labelledby="project-information-title" onCancel={event => { event.preventDefault(); onClose(); }}>
    <header><h2 id="project-information-title">作品信息</h2><button type="button" aria-label="关闭作品信息" onClick={onClose}>×</button></header>
    <dl className="project-information-metadata"><div><dt>作品名称</dt><dd>{project.title}</dd></div><div><dt>一句话简介</dt><dd>{project.logline.trim() ? project.logline : "尚未填写"}</dd></div></dl>
    <section aria-labelledby="project-information-goals"><h3 id="project-information-goals">创作目标</h3>
      {!reading ? <p role="status">正在读取创作简报…</p> : reading.status === "error" ? <div role="alert"><p>{reading.message}</p><button type="button" onClick={() => setRetry(value => value + 1)}>重新读取作品信息</button></div>
        : <><p className="project-information-source">{reading.source === "adopted" ? "来自已采用的创作简报" : reading.source === "saved" ? "来自已保存的创作简报，尚未采用" : "尚未填写创作简报"}{reading.newer && "；另有未采用修改"}</p>{reading.source !== "empty" && <dl>{reading.fields.map(field => <div key={field.label}><dt>{field.label}</dt><dd>{field.text}</dd></div>)}</dl>}</>}
      <button type="button" onClick={() => { onClose(); onOpenBrief(); }}>查看作品概况</button>
    </section>
    {api && <section><h3>创作回看</h3><p>作品偏好、保留亮点、试写与读者反馈。</p><button type="button" aria-expanded={qualityOpen} onClick={() => setQualityOpen(value => !value)}>{qualityOpen ? "收起创作回看" : "打开创作回看"}</button>{qualityOpen && <QualityNotebook key={project.id} api={api} projectId={project.id} />}</section>}
  </dialog>;
}
