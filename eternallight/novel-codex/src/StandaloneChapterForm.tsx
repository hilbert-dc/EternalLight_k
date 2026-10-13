import { useEffect, useRef, useState } from "react";

export function StandaloneChapterForm({ nextNumber, disabled, busy, onCreate, onCancel, volumeTitle, requestId = 0 }: {
  requestId?: number;
  volumeTitle?: string;
  nextNumber: number;
  disabled: boolean;
  busy: boolean;
  onCreate: (title: string) => Promise<boolean>;
  onCancel?: () => void;
}) {
  const [title, setTitle] = useState("");
  const submitting = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!requestId) return;
    const frame = requestAnimationFrame(() => { inputRef.current?.scrollIntoView({ block: "center" }); inputRef.current?.focus({ preventScroll: true }); });
    return () => cancelAnimationFrame(frame);
  }, [requestId]);
  return <form className="empty-chapter-editor" aria-label={volumeTitle ? "在卷中创建章节" : "创建独立章节"} onSubmit={event => {
    event.preventDefault();
    if (disabled || busy || submitting.current) return;
    submitting.current = true;
    void onCreate(title.trim()).finally(() => { submitting.current = false; });
  }}>
    <div><h3>{nextNumber === 1 ? "直接写第一章" : `直接写第 ${nextNumber} 章`}</h3><p>{volumeTitle ? `新章节将加入「${volumeTitle}」，创建后即可开始写作。` : "创建后可直接手写，也可让引擎续写；稍后再关联章纲。"}</p></div>
    <label><span>章节标题（可选）</span><input ref={inputRef} autoFocus aria-label="新章节标题" placeholder={`第 ${nextNumber} 章`} maxLength={120} disabled={disabled || busy} value={title} onChange={event => setTitle(event.target.value)} /></label>
    <div>{onCancel && <button className="secondary-button" type="button" disabled={busy} onClick={onCancel}>取消新建章节</button>}<button className="primary-button" type="submit" disabled={disabled || busy}>{busy ? "正在创建…" : volumeTitle ? "创建章节" : "创建独立章节"}</button></div>
  </form>;
}
