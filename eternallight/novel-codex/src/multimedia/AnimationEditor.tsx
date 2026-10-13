import { useEffect, useRef, useState } from "react";
import type { AnimationClip, AnimationDocument, AnimationShot, AnimationTrack, MediaAsset, MediaSelector, MultimediaApi } from "../../shared/multimedia-types";
import { AssetChoice, AssetReferences, NumberField } from "./EditorControls";
import { AnimationPreview } from "./AnimationPreview";
import { readMediaView, saveMediaView } from "./view-state";
import { MediaPreview } from "./MediaPreview";
import { animationLength, changeAnimation, clipEnd, formatTime, frameSeconds, newMediaId, secondsTime, timeSeconds } from "./editor-helpers";

export function AnimationEditor({ document, viewStorageKey, assets, disabled, onChange, onSelection, onImport, readSubtitleFont }: {
  document: AnimationDocument; viewStorageKey: string; assets: readonly MediaAsset[]; disabled: boolean;
  onChange: (document: AnimationDocument) => void;
  onSelection: (selection: { label: string; assetRevisionId: string | null; selector: MediaSelector } | null) => void;
  onImport: () => void;
  readSubtitleFont: MultimediaApi["readSubtitleFont"];
}) {
  const bookmark = useRef(readMediaView(viewStorageKey)).current;
  const [shotId, setShotId] = useState<string | null>(typeof bookmark.shotId === "string" ? bookmark.shotId : document.shots[0]?.id ?? null);
  const [clipId, setClipId] = useState<string | null>(typeof bookmark.clipId === "string" ? bookmark.clipId : null);
  const [timelineOpen, setTimelineOpen] = useState(bookmark.timelineOpen === true);
  const notify = useRef(onSelection); notify.current = onSelection;
  const shot = document.shots.find(item => item.id === shotId);
  const track = document.timeline.tracks.find(item => item.clips.some(clip => clip.id === clipId));
  const clip = track?.clips.find(item => item.id === clipId);
  useEffect(() => { saveMediaView(viewStorageKey, { shotId: shot?.id ?? null, clipId: clip?.id ?? null, timelineOpen }); }, [viewStorageKey, shot?.id, clip?.id, timelineOpen]);
  const asset = assets.find(item => item.assetRevisionId === (clip?.assetRevisionId ?? (clip ? document.shots.find(item => item.id === clip.shotId)?.assetRevisionId : shot?.assetRevisionId)));
  const referencedShot = clip ? document.shots.find(item => item.id === clip.shotId) : undefined;
  const locked = disabled || Boolean(clip ? referencedShot?.locked : shot?.locked);
  useEffect(() => {
    if (clip && track) notify.current({ label: `${track.title} · ${clip.caption || referencedShot?.title || "片段"}`, assetRevisionId: asset?.assetRevisionId ?? null, selector: { kind: "clip", trackId: track.id, clipId: clip.id, range: { start: clip.sourceStart, duration: clip.duration } } });
    else if (shot) notify.current({ label: shot.title || "镜头", assetRevisionId: shot.assetRevisionId, selector: { kind: "shot", shotId: shot.id } });
    else notify.current(null);
  }, [shot, clip, track, asset, referencedShot]);
  function updateShot(change: (shot: AnimationShot) => void) {
    if (!shot || locked) return;
    onChange(changeAnimation(document, draft => { change(draft.shots.find(item => item.id === shot.id)!); }));
  }
  function updateClip(change: (clip: AnimationClip) => void) {
    if (!clip || !track || locked) return;
    onChange(changeAnimation(document, draft => { change(draft.timeline.tracks.find(item => item.id === track.id)!.clips.find(item => item.id === clip.id)!); }));
  }
  function addShot() {
    const id = newMediaId("shot"); onChange(changeAnimation(document, draft => { draft.shots.push({ id, title: `镜头 ${draft.shots.length + 1}`, description: "", assetRevisionId: null, referenceAssetRevisionIds: [], locked: false }); })); setShotId(id); setClipId(null);
  }
  function appendClip(kind: AnimationTrack["kind"], existingTrackId?: string) {
    const id = newMediaId("clip");
    onChange(changeAnimation(document, draft => {
      let destination = existingTrackId ? draft.timeline.tracks.find(item => item.id === existingTrackId) : draft.timeline.tracks.find(item => item.kind === kind);
      if (!destination) { destination = { id: newMediaId("track"), title: kind === "video" ? "画面" : kind === "audio" ? "声音" : "字幕", kind, muted: false, clips: [] }; draft.timeline.tracks.push(destination); }
      const picture = kind === "video" ? assets.find(item => item.assetRevisionId === shot?.assetRevisionId) : undefined;
      destination.clips.push({ id, shotId: kind === "video" ? shot?.id ?? null : null, assetRevisionId: kind === "video" ? picture?.assetRevisionId ?? null : null, start: secondsTime(Math.max(0, ...destination.clips.map(clipEnd)), draft.frameRate), duration: secondsTime(picture?.duration ? Math.max(frameSeconds(draft.frameRate), timeSeconds(picture.duration)) : 3, draft.frameRate), sourceStart: secondsTime(0, draft.frameRate), playbackRate: { numerator: 1, denominator: 1 }, caption: "" });
    })); setClipId(id); setTimelineOpen(true);
  }
  function addTrack(kind: AnimationTrack["kind"]) {
    onChange(changeAnimation(document, draft => { draft.timeline.tracks.push({ id: newMediaId("track"), title: `${kind === "video" ? "画面" : kind === "audio" ? "声音" : "字幕"} ${draft.timeline.tracks.filter(item => item.kind === kind).length + 1}`, kind, muted: false, clips: [] }); })); setTimelineOpen(true);
  }
  const total = Math.max(10, animationLength(document));
  return <div className="mm-animation-editor">
    <AnimationPreview viewStorageKey={`${viewStorageKey}:playback`} document={document} assets={assets} readSubtitleFont={readSubtitleFont} />
    <nav className="mm-object-strip" aria-label="动画镜头">{document.shots.map((item, index) => <button type="button" key={item.id} aria-pressed={shot?.id === item.id && !clip} onClick={() => { setShotId(item.id); setClipId(null); }}>{item.title || `镜头 ${index + 1}`}</button>)}<button type="button" disabled={disabled} onClick={addShot}>＋ 添加镜头</button></nav>
    {!document.shots.length && !document.timeline.tracks.length && <div className="mm-empty"><h3>从一个镜头开始</h3><p>直接在对话中描述动画，或导入自己的画面和声音。</p></div>}
    <details className="mm-timeline" open={timelineOpen} onToggle={event => setTimelineOpen(event.currentTarget.open)}><summary>时间轴 · {formatTime(animationLength(document))}</summary>
      <div className="mm-timeline-tracks">{document.timeline.tracks.map(item => <section className="mm-track" key={item.id} aria-label={`${item.title}轨道`}><div className="mm-track-heading"><span>{item.title}</span><button type="button" disabled={disabled} aria-pressed={item.muted} onClick={() => onChange(changeAnimation(document, draft => { const target = draft.timeline.tracks.find(value => value.id === item.id)!; target.muted = !target.muted; }))}>{item.muted ? "启用" : item.kind === "audio" ? "静音" : "隐藏"}</button><button type="button" disabled={disabled} onClick={() => appendClip(item.kind, item.id)}>＋ 片段</button></div><div className="mm-track-clips">{item.clips.map((fragment, index) => <button type="button" key={fragment.id} className={clip?.id === fragment.id ? "selected" : ""} aria-pressed={clip?.id === fragment.id} aria-label={`${item.title}片段 ${index + 1}`} style={{ left: `${timeSeconds(fragment.start) / total * 100}%`, width: `${Math.max(timeSeconds(fragment.duration) / total * 100, 2)}%` }} onClick={() => { setClipId(fragment.id); if (fragment.shotId) setShotId(fragment.shotId); }}>{fragment.caption || document.shots.find(value => value.id === fragment.shotId)?.title || assets.find(value => value.assetRevisionId === fragment.assetRevisionId)?.name || "未命名片段"}</button>)}</div></section>)}</div>
      <div className="mm-inline-actions"><button type="button" disabled={disabled} onClick={() => appendClip("video")}>＋ 画面片段</button><button type="button" disabled={disabled} onClick={() => appendClip("audio")}>＋ 声音片段</button><button type="button" disabled={disabled} onClick={() => appendClip("subtitle")}>＋ 字幕</button></div>
      <details className="mm-track-options"><summary>轨道设置</summary><div className="mm-inline-actions"><button type="button" disabled={disabled} onClick={() => addTrack("video")}>＋ 画面轨道</button><button type="button" disabled={disabled} onClick={() => addTrack("audio")}>＋ 声音轨道</button><button type="button" disabled={disabled} onClick={() => addTrack("subtitle")}>＋ 字幕轨道</button></div>{document.timeline.tracks.map(item => <div key={item.id} className="mm-track-option"><label className="mm-field"><span>轨道名称</span><input value={item.title} disabled={disabled} onChange={event => onChange(changeAnimation(document, draft => { draft.timeline.tracks.find(value => value.id === item.id)!.title = event.target.value; }))} /></label><button type="button" disabled={disabled || item.clips.length > 0} onClick={() => onChange(changeAnimation(document, draft => { draft.timeline.tracks = draft.timeline.tracks.filter(value => value.id !== item.id); }))}>删除空轨道</button></div>)}</details>
    </details>
    {(shot || clip) && <section className="mm-animation-inspector"><div className="mm-inspector-heading"><h3>{clip ? `${track?.title}片段` : shot?.title}</h3>{!clip && shot && <button type="button" disabled={disabled} aria-pressed={shot.locked} onClick={() => onChange(changeAnimation(document, draft => { const target = draft.shots.find(item => item.id === shot.id)!; target.locked = !target.locked; }))}>{shot.locked ? "取消保护" : "保护镜头"}</button>}</div>
      <fieldset disabled={locked} className="mm-fields">
        {clip && track ? <>
          {track.kind === "subtitle" ? <label className="mm-field"><span>字幕文字</span><textarea value={clip.caption} onChange={event => updateClip(item => { item.caption = event.target.value; })} /></label> : <AssetChoice assets={assets} kinds={track.kind === "audio" ? ["audio", "video"] : ["image", "video"]} label={track.kind === "audio" ? "声音素材" : "片段画面"} value={clip.assetRevisionId ?? referencedShot?.assetRevisionId ?? null} onChange={value => updateClip(item => { item.assetRevisionId = value; })} />}
          <div className="mm-field-row"><NumberField label="出现时间（秒）" value={timeSeconds(clip.start)} step={.01} onChange={value => updateClip(item => { item.start = secondsTime(value, document.frameRate); })} /><NumberField label="片段时长（秒）" value={timeSeconds(clip.duration)} step={.01} min={frameSeconds(document.frameRate)} max={asset?.duration ? Math.max(frameSeconds(document.frameRate), (timeSeconds(asset.duration) - timeSeconds(clip.sourceStart)) * clip.playbackRate.denominator / clip.playbackRate.numerator) : undefined} onChange={value => updateClip(item => { item.duration = secondsTime(value, document.frameRate); })} />{track.kind !== "subtitle" && <NumberField label="素材起点（秒）" value={timeSeconds(clip.sourceStart)} step={.01} max={asset?.duration ? Math.max(0, timeSeconds(asset.duration) - timeSeconds(clip.duration) * clip.playbackRate.numerator / clip.playbackRate.denominator) : undefined} onChange={value => updateClip(item => { item.sourceStart = secondsTime(value, asset?.duration?.rate ?? document.frameRate); })} />}</div>
          {track.kind !== "subtitle" && <NumberField label="播放速度" value={clip.playbackRate.numerator / clip.playbackRate.denominator} min={.1} max={Math.min(4, timeSeconds(clip.duration) * clip.playbackRate.numerator / clip.playbackRate.denominator / frameSeconds(document.frameRate))} step={.1} onChange={value => updateClip(item => { const speed = { numerator: Math.round(value * 1000), denominator: 1000 }; const preservedSource = timeSeconds(item.duration) * item.playbackRate.numerator / item.playbackRate.denominator; item.playbackRate = speed; item.duration = { value: Math.max(1, Math.floor(preservedSource / value / frameSeconds(document.frameRate))), rate: document.frameRate }; })} />}
          <label className="mm-field"><span>所在轨道</span><select value={track.id} onChange={event => onChange(changeAnimation(document, draft => { const origin = draft.timeline.tracks.find(item => item.id === track.id)!, destination = draft.timeline.tracks.find(item => item.id === event.target.value)!; const current = origin.clips.find(item => item.id === clip.id)!; origin.clips = origin.clips.filter(item => item.id !== clip.id); destination.clips.push(current); }))}>{document.timeline.tracks.filter(item => item.kind === track.kind).map(item => <option key={item.id} value={item.id}>{item.title}</option>)}</select></label>
          <div className="mm-inline-actions"><button type="button" onClick={() => onChange(changeAnimation(document, draft => { const target = draft.timeline.tracks.find(item => item.id === track.id)!; target.clips.push({ ...structuredClone(clip), id: newMediaId("clip"), start: secondsTime(Math.max(0, ...target.clips.map(clipEnd)), document.frameRate) }); }))}>复制片段</button><button type="button" onClick={() => { onChange(changeAnimation(document, draft => { const target = draft.timeline.tracks.find(item => item.id === track.id)!; target.clips = target.clips.filter(item => item.id !== clip.id); })); setClipId(null); }}>删除片段</button></div>
        </> : shot && <>
          <label className="mm-field"><span>镜头名称</span><input value={shot.title} onChange={event => updateShot(item => { item.title = event.target.value; })} /></label><label className="mm-field"><span>镜头说明</span><textarea value={shot.description} rows={3} onChange={event => updateShot(item => { item.description = event.target.value; })} /></label><AssetChoice assets={assets} kinds={["image", "video"]} value={shot.assetRevisionId} onChange={value => updateShot(item => { item.assetRevisionId = value; })} /><div className="mm-inline-actions"><button type="button" onClick={onImport}>导入素材</button><button type="button" onClick={() => appendClip("video")}>加入时间轴</button><button type="button" disabled={document.timeline.tracks.some(item => item.clips.some(clip => clip.shotId === shot.id))} onClick={() => { onChange(changeAnimation(document, draft => { draft.shots = draft.shots.filter(item => item.id !== shot.id); })); setShotId(null); }}>删除镜头</button></div>
        </>}
        {!clip && shot && <AssetReferences assets={assets} value={shot.referenceAssetRevisionIds} onChange={value => updateShot(item => { item.referenceAssetRevisionIds = value; })} />}
      </fieldset>
      {asset && <details><summary>查看原素材</summary><MediaPreview asset={asset} /></details>}
    </section>}
    <details className="mm-edition-settings"><summary>画面设置</summary><fieldset disabled={disabled} className="mm-field-row"><NumberField label="画面宽度" min={16} max={8192} value={document.width} onChange={value => onChange({ ...document, width: Math.round(value) })} /><NumberField label="画面高度" min={16} max={8192} value={document.height} onChange={value => onChange({ ...document, height: Math.round(value) })} /><label className="mm-field"><span>帧率</span><select value={`${document.frameRate.numerator}/${document.frameRate.denominator}`} onChange={event => { const [numerator, denominator] = event.target.value.split("/").map(Number); onChange({ ...document, frameRate: { numerator, denominator } }); }}>{[[24, 1], [25, 1], [30, 1], [30000, 1001], [60, 1]].map(([n, d]) => <option key={`${n}/${d}`} value={`${n}/${d}`}>{(n / d).toFixed(d === 1 ? 0 : 2)} 帧 / 秒</option>)}</select></label></fieldset></details>
  </div>;
}
