import { useEffect, useRef, useState } from "react";
import type { ComicDocument, ComicPanel, MediaAsset, MediaRect, MediaSelector } from "../../shared/multimedia-types";
import { AssetChoice, AssetReferences, NumberField } from "./EditorControls";
import { readMediaView, saveMediaView } from "./view-state";
import { MediaPreview } from "./MediaPreview";
import { boundedRect, changeComic, newMediaId } from "./editor-helpers";

export function ComicEditor({ document, viewStorageKey, assets, disabled, onChange, onSelection, onImport }: {
  document: ComicDocument; viewStorageKey: string; assets: readonly MediaAsset[]; disabled: boolean;
  onChange: (document: ComicDocument) => void;
  onSelection: (selection: { label: string; assetRevisionId: string | null; selector: MediaSelector } | null) => void;
  onImport: () => void;
}) {
  const bookmark = useRef(readMediaView(viewStorageKey)).current;
  const [pageId, setPageId] = useState(typeof bookmark.pageId === "string" ? bookmark.pageId : document.pages[0]?.id ?? "");
  const [panelId, setPanelId] = useState<string | null>(typeof bookmark.panelId === "string" ? bookmark.panelId : null);
  const [region, setRegion] = useState<MediaRect | null>(null);
  const regionOwner = useRef("");
  const [zoom, setZoom] = useState(typeof bookmark.zoom === "number" && [50, 75, 100, 150, 200].includes(bookmark.zoom) ? bookmark.zoom : 100);
  const canvas = useRef<HTMLDivElement>(null);
  const dragging = useRef<{ id: string; x: number; y: number; rect: MediaRect; scale: number } | null>(null);
  const page = document.pages.find(item => item.id === pageId) ?? document.pages[0];
  const panel = page?.panels.find(item => item.id === panelId);
  useEffect(() => { saveMediaView(viewStorageKey, { pageId: page?.id ?? null, panelId: panel?.id ?? null, zoom }); }, [viewStorageKey, page?.id, panel?.id, zoom]);
  const asset = assets.find(item => item.assetRevisionId === panel?.assetRevisionId);
  const selectionIdentity = `${page?.id}:${panel?.id}:${panel?.assetRevisionId}`;
  const currentRegion = regionOwner.current === selectionIdentity ? region : null;
  const notify = useRef(onSelection); notify.current = onSelection;
  useEffect(() => { setRegion(null); }, [page?.id, panel?.id, panel?.assetRevisionId]);
  useEffect(() => {
    if (!page || !panel) { notify.current(null); return; }
    notify.current({ label: `${page.title || `第 ${document.pages.indexOf(page) + 1} 页`} · 第 ${page.panels.indexOf(panel) + 1} 格${currentRegion ? "选区" : ""}`, assetRevisionId: panel.assetRevisionId, selector: { kind: "panel", pageId: page.id, panelId: panel.id, ...(currentRegion ? { region: currentRegion } : {}) } });
  }, [page, panel, currentRegion, document.pages]);
  function updatePanel(change: (panel: ComicPanel) => void) {
    if (disabled || !panel || panel.locked || !page) return;
    onChange(changeComic(document, draft => { const target = draft.pages.find(item => item.id === page.id)?.panels.find(item => item.id === panel.id); if (target) change(target); }));
  }
  function addPage() {
    const id = newMediaId("page"); onChange(changeComic(document, draft => { draft.pages.push({ id, title: `第 ${draft.pages.length + 1} 页`, width: 1200, height: 1600, readingDirection: "leftToRight", panels: [] }); })); setPageId(id); setPanelId(null);
  }
  function addPanel() {
    if (!page) return;
    const id = newMediaId("panel"); onChange(changeComic(document, draft => {
      const target = draft.pages.find(item => item.id === page.id)!;
      const width = Math.max(1, Math.floor((target.width - 100) / 2)); const index = target.panels.length;
      const rect = { x: 30 + index % 2 * (width + 40), y: 30 + Math.floor(index / 2) * 520, width, height: 480 };
      target.height = Math.max(target.height, rect.y + rect.height + 30);
      target.panels.push({ id, description: "", rect, assetRevisionId: null, referenceAssetRevisionIds: [], balloons: [], locked: false });
    })); setPanelId(id);
  }
  function movePanel(offset: number) {
    if (!page || !panel) return;
    const index = page.panels.indexOf(panel), next = index + offset;
    if (next < 0 || next >= page.panels.length || page.panels[next].locked) return;
    onChange(changeComic(document, draft => { const panels = draft.pages.find(item => item.id === page.id)!.panels; [panels[index], panels[next]] = [panels[next], panels[index]]; }));
  }
  return <div className="mm-comic-editor">
    <nav className="mm-object-strip" aria-label="漫画页面">{document.pages.map((item, index) => <button type="button" key={item.id} aria-pressed={page?.id === item.id} onClick={() => { setPageId(item.id); setPanelId(null); }}>{item.title || `第 ${index + 1} 页`}</button>)}<button type="button" disabled={disabled} onClick={addPage}>＋ 添加页面</button></nav>
    {!page ? <div className="mm-empty"><h3>从一页漫画开始</h3><p>在对话中描述故事，或添加页面后放入自己的画稿。</p></div> : <>
      <div className="mm-editor-toolbar"><label className="mm-inline-field"><span>页面名称</span><input value={page.title} disabled={disabled} onChange={event => onChange(changeComic(document, draft => { draft.pages.find(item => item.id === page.id)!.title = event.target.value; }))} /></label><label className="mm-inline-field"><span>缩放</span><select value={zoom} onChange={event => setZoom(Number(event.target.value))}>{[50, 75, 100, 150, 200].map(value => <option key={value} value={value}>{value}%</option>)}</select></label><button type="button" disabled={disabled} onClick={addPanel}>＋ 添加画格</button></div>
      <div className="mm-editor-columns">
        <div className="mm-canvas-scroll"><div ref={canvas} className="mm-comic-page" aria-label={page.title || "漫画页面"} style={{ aspectRatio: `${page.width}/${page.height}`, width: `${zoom}%` }}>
          {!page.panels.length && <p className="mm-canvas-empty">添加画格，或在对话中描述这一页。</p>}
          {page.panels.map((item, index) => {
            const picture = assets.find(candidate => candidate.assetRevisionId === item.assetRevisionId);
            return <button type="button" key={item.id} className={`mm-comic-panel${item.id === panel?.id ? " selected" : ""}`} data-panel-id={item.id} aria-label={`第 ${index + 1} 格${item.locked ? "，已保护" : ""}`} aria-pressed={item.id === panel?.id}
              style={{ left: `${item.rect.x / page.width * 100}%`, top: `${item.rect.y / page.height * 100}%`, width: `${item.rect.width / page.width * 100}%`, height: `${item.rect.height / page.height * 100}%` }}
              onClick={() => setPanelId(item.id)} onPointerDown={event => { setPanelId(item.id); if (disabled || item.locked || event.button !== 0) return; const bounds = canvas.current?.getBoundingClientRect(); if (!bounds?.width) return; event.currentTarget.setPointerCapture(event.pointerId); dragging.current = { id: item.id, x: event.clientX, y: event.clientY, rect: { ...item.rect }, scale: page.width / bounds.width }; }}
              onPointerMove={event => { const start = dragging.current; if (!start || start.id !== item.id || !event.buttons) return; const dx = (event.clientX - start.x) * start.scale, dy = (event.clientY - start.y) * start.scale; if (Math.abs(dx) + Math.abs(dy) < 3) return; onChange(changeComic(document, draft => { const target = draft.pages.find(p => p.id === page.id)!.panels.find(p => p.id === item.id)!; target.rect = { ...start.rect, x: Math.round(Math.min(page.width - start.rect.width, Math.max(0, start.rect.x + dx))), y: Math.round(Math.min(page.height - start.rect.height, Math.max(0, start.rect.y + dy))) }; })); }}
              onPointerUp={() => { dragging.current = null; }} onPointerCancel={() => { dragging.current = null; }}>
              {picture ? <img src={picture.previewUrl} alt={item.description || picture.name} draggable={false} /> : <span className="mm-panel-placeholder">{item.description || `第 ${index + 1} 格`}</span>}
              {item.balloons.map(balloon => <span key={balloon.id} className={`mm-balloon ${balloon.kind}`} style={{ left: `${balloon.rect.x / item.rect.width * 100}%`, top: `${balloon.rect.y / item.rect.height * 100}%`, width: `${balloon.rect.width / item.rect.width * 100}%`, minHeight: `${balloon.rect.height / item.rect.height * 100}%` }}>{balloon.text}</span>)}
            </button>;
          })}
        </div></div>
        {panel ? <section className="mm-inspector" aria-label="画格编辑"><div className="mm-inspector-heading"><h3>第 {page.panels.indexOf(panel) + 1} 格</h3><button type="button" disabled={disabled} aria-pressed={panel.locked} onClick={() => onChange(changeComic(document, draft => { const item = draft.pages.find(item => item.id === page.id)!.panels.find(item => item.id === panel.id)!; item.locked = !item.locked; }))}>{panel.locked ? "取消保护" : "保护画格"}</button></div>
          <fieldset disabled={disabled || panel.locked} className="mm-fields">
            <label className="mm-field"><span>画面说明</span><textarea value={panel.description} rows={3} onChange={event => updatePanel(item => { item.description = event.target.value; })} /></label>
            <AssetChoice assets={assets} kinds={["image"]} value={panel.assetRevisionId} onChange={value => updatePanel(item => { item.assetRevisionId = value; })} />
            <button type="button" onClick={onImport}>导入画面</button>
            <AssetReferences assets={assets} value={panel.referenceAssetRevisionIds} onChange={value => updatePanel(item => { item.referenceAssetRevisionIds = value; })} />
            <div className="mm-field-row">{(["x", "y", "width", "height"] as const).map((key, index) => <NumberField key={key} label={["横向位置", "纵向位置", "画格宽度", "画格高度"][index]} value={panel.rect[key]} max={key === "x" || key === "width" ? page.width : page.height} min={key === "x" || key === "y" ? 0 : 1} onChange={value => updatePanel(item => { item.rect = boundedRect({ ...item.rect, [key]: value }, page.width, page.height); item.balloons = item.balloons.map(balloon => ({ ...balloon, rect: boundedRect(balloon.rect, item.rect.width, item.rect.height) })); })} />)}</div>
            <div className="mm-inline-actions"><button type="button" disabled={page.panels.indexOf(panel) === 0 || page.panels[page.panels.indexOf(panel) - 1]?.locked} onClick={() => movePanel(-1)}>提前阅读</button><button type="button" disabled={page.panels.indexOf(panel) === page.panels.length - 1 || page.panels[page.panels.indexOf(panel) + 1]?.locked} onClick={() => movePanel(1)}>延后阅读</button><button type="button" onClick={() => { onChange(changeComic(document, draft => { const target = draft.pages.find(item => item.id === page.id)!; target.panels = target.panels.filter(item => item.id !== panel.id); })); setPanelId(null); }}>删除画格</button></div>
            <div className="mm-inspector-heading"><h4>对白与文字</h4><button type="button" onClick={() => updatePanel(item => { item.balloons.push({ id: newMediaId("balloon"), text: "", speaker: "", kind: "speech", rect: boundedRect({ x: 20, y: 20 + item.balloons.length * 100, width: item.rect.width * .65, height: 80 }, item.rect.width, item.rect.height) }); })}>＋ 添加对白</button></div>
            {panel.balloons.map((balloon, index) => <div key={balloon.id} className="mm-balloon-editor"><label className="mm-field"><span>对白 {index + 1}</span><textarea value={balloon.text} rows={2} onChange={event => updatePanel(item => { item.balloons.find(value => value.id === balloon.id)!.text = event.target.value; })} /></label><label className="mm-field"><span>说话者</span><input value={balloon.speaker} onChange={event => updatePanel(item => { item.balloons.find(value => value.id === balloon.id)!.speaker = event.target.value; })} /></label><label className="mm-field"><span>文字样式</span><select value={balloon.kind} onChange={event => updatePanel(item => { item.balloons.find(value => value.id === balloon.id)!.kind = event.target.value as typeof balloon.kind; })}><option value="speech">对白气泡</option><option value="thought">内心想法</option><option value="caption">旁白</option><option value="sound">音效文字</option></select></label><details><summary>文字位置与大小</summary><div className="mm-field-row">{(["x", "y", "width", "height"] as const).map((key, offset) => <NumberField key={key} label={`对白${["横向位置", "纵向位置", "宽度", "高度"][offset]}`} value={balloon.rect[key]} min={offset < 2 ? 0 : 1} onChange={value => updatePanel(item => { const target = item.balloons.find(value => value.id === balloon.id)!; target.rect = boundedRect({ ...target.rect, [key]: value }, item.rect.width, item.rect.height); })} />)}</div></details><button type="button" onClick={() => updatePanel(item => { item.balloons = item.balloons.filter(value => value.id !== balloon.id); })}>删除这条文字</button></div>)}
          </fieldset>
          {asset && <MediaPreview asset={asset} region={currentRegion} onRegionChange={next => { regionOwner.current = selectionIdentity; setRegion(next); }} />}
        </section> : <section className="mm-inspector mm-selection-hint"><p>选择画格后，可以调整画面、对白和排版。</p><details><summary>页面设置</summary><div className="mm-fields"><label className="mm-field"><span>阅读方向</span><select disabled={disabled} value={page.readingDirection} onChange={event => onChange(changeComic(document, draft => { draft.pages.find(item => item.id === page.id)!.readingDirection = event.target.value as typeof page.readingDirection; }))}><option value="leftToRight">从左到右</option><option value="rightToLeft">从右到左</option><option value="vertical">从上到下</option></select></label><NumberField label="页面宽度" value={page.width} disabled={disabled} min={Math.max(100, ...page.panels.map(item => item.rect.x + item.rect.width))} onChange={value => onChange(changeComic(document, draft => { draft.pages.find(item => item.id === page.id)!.width = Math.round(value); }))} /><NumberField label="页面高度" value={page.height} disabled={disabled} min={Math.max(100, ...page.panels.map(item => item.rect.y + item.rect.height))} onChange={value => onChange(changeComic(document, draft => { draft.pages.find(item => item.id === page.id)!.height = Math.round(value); }))} /><button type="button" disabled={disabled || page.panels.some(item => item.locked)} onClick={() => { onChange(changeComic(document, draft => { draft.pages = draft.pages.filter(item => item.id !== page.id); })); setPanelId(null); }}>删除页面</button></div></details></section>}
      </div>
    </>}
  </div>;
}
