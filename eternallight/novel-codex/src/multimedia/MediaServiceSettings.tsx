import { useEffect, useRef, useState } from "react";
import type { MediaServiceConnection, MultimediaApi } from "../../shared/multimedia-types";
import { newMediaId } from "./editor-helpers";
import "./multimedia.css";

const labels: Record<MediaServiceConnection["protocol"], string> = { "openai-images": "图片生成与编辑", "openai-audio": "配音与转录", runway: "视频生成" };
export function MediaServiceSettings({ api, onClose }: { api: MultimediaApi; onClose?: () => void }) {
  const [connections, setConnections] = useState<MediaServiceConnection[]>([]), [selectedId, setSelectedId] = useState<string | null>(null);
  const [title, setTitle] = useState(""), [protocol, setProtocol] = useState<MediaServiceConnection["protocol"]>("openai-images");
  const [baseUrl, setBaseUrl] = useState(""), [model, setModel] = useState(""), [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null), [notice, setNotice] = useState<string | null>(null), [pending, setPending] = useState(false);
  const [loading, setLoading] = useState(true);
  const alive = useRef(true), submitted = useRef(false);
  useEffect(() => { alive.current = true; void api.listConnections().then(value => { if (alive.current) setConnections(value.connections); }).catch(reason => { if (alive.current) setError(reason instanceof Error ? reason.message : "无法读取媒体服务。"); }).finally(() => { if (alive.current) setLoading(false); }); return () => { alive.current = false; }; }, [api]);
  function edit(item: MediaServiceConnection | null) { setSelectedId(item?.id ?? null); setTitle(item?.title ?? ""); setProtocol(item?.protocol ?? "openai-images"); setBaseUrl(item?.baseUrl ?? ""); setModel(item?.model ?? ""); setApiKey(""); setError(null); setNotice(null); }
  async function save(event: React.FormEvent) {
    event.preventDefault(); if (submitted.current) return;
    if (!title.trim() || !baseUrl.trim() || !model.trim()) { setError("请填写服务名称、地址和模型。"); return; }
    submitted.current = true; setPending(true); setError(null); setNotice(null);
    try { const value = await api.saveConnection({ id: selectedId ?? newMediaId("connection"), title: title.trim(), protocol, baseUrl: baseUrl.trim(), model: model.trim(), ...(apiKey ? { apiKey } : {}) }); if (!alive.current) return; setConnections(current => [...current.filter(item => item.id !== value.id), value]); setSelectedId(value.id); setApiKey(""); setNotice("连接已保存。可用能力以服务验证结果为准。"); }
    catch (reason) { if (alive.current) setError(reason instanceof Error ? reason.message : "连接未保存，请重试。"); }
    finally { submitted.current = false; if (alive.current) setPending(false); }
  }
  const selected = connections.find(item => item.id === selectedId);
  return <section className="mm-service-settings" aria-label="媒体服务"><header className="mm-inspector-heading"><h2>媒体服务</h2>{onClose && <button type="button" disabled={pending} onClick={onClose}>关闭</button>}</header><p>为图片、声音和视频配置可用服务。创作时可直接在对话中提出要求。</p>
    {loading ? <p role="status">正在读取连接…</p> : <nav className="mm-object-strip" aria-label="媒体连接">{connections.map(item => <button type="button" disabled={pending} key={item.id} aria-pressed={selectedId === item.id} onClick={() => edit(item)}>{item.title}</button>)}<button type="button" disabled={pending} onClick={() => edit(null)}>＋ 添加连接</button></nav>}
    <form className="mm-fields" onSubmit={event => void save(event)}><fieldset disabled={pending || loading} className="mm-fields"><label className="mm-field"><span>服务名称</span><input value={title} onChange={event => setTitle(event.target.value)} maxLength={120} /></label><label className="mm-field"><span>服务类型</span><select value={protocol} onChange={event => setProtocol(event.target.value as typeof protocol)}>{Object.entries(labels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label><label className="mm-field"><span>API 地址</span><input value={baseUrl} type="url" placeholder="https://…" onChange={event => setBaseUrl(event.target.value)} autoComplete="off" /></label><label className="mm-field"><span>模型</span><input value={model} onChange={event => setModel(event.target.value)} autoComplete="off" /></label><label className="mm-field"><span>API 密钥{selected?.credentialConfigured ? "（已设置，留空则保留）" : ""}</span><input value={apiKey} type="password" onChange={event => setApiKey(event.target.value)} autoComplete="new-password" spellCheck={false} /></label><button type="submit" className="primary-button">{pending ? "保存中…" : "保存连接"}</button></fieldset></form>
    {error && <p className="mm-error" role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
    {selected?.capabilities.length ? <ul className="mm-capabilities">{selected.capabilities.map((item, index) => <li key={`${item.kind}:${index}`}>{({ "image.generate": "生成图片", "image.edit": "修改图片", "audio.speech": "生成配音", "audio.transcribe": "音频转录", "video.generate": "生成视频", "local.render": "本地合成" })[item.kind]}：{item.available ? "可用" : item.reason || "尚不可用"}</li>)}</ul> : null}
  </section>;
}
