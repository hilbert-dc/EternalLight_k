import { randomUUID } from "node:crypto";

/** BrowserWindow remains an IO adapter; callers supply only renderer-produced
 * HTML with inline styles and embedded raster assets. No page code can run. */
export async function renderMediaHtml({ html, width, height, format, signal, purpose, subtitleNodeIds }, { BrowserWindow, session }) {
  signal?.throwIfAborted();
  if (typeof html !== "string" || Buffer.byteLength(html) > 80 * 1024 * 1024
    || !Number.isInteger(width) || !Number.isInteger(height)
    || width < 1 || height < 1 || width > 16_384 || height > 32_768 || width * height > 100_000_000
    || !["png", "pdf"].includes(format) || purpose !== undefined && purpose !== "subtitle") throw new Error("页面渲染参数无效");
  const subtitle = purpose === "subtitle";
  if (subtitle && (format !== "png" || !Array.isArray(subtitleNodeIds) || !subtitleNodeIds.length || subtitleNodeIds.length > 16
    || subtitleNodeIds.some((id, index) => id !== `subtitle-${index}`))) throw Error("字幕渲染参数无效");
  const nonce = randomUUID(), renderUrl = `https://media-render.invalid/${nonce}`;
  const renderSession = session.fromPartition(`media-render-${nonce}`);
  renderSession.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  renderSession.webRequest.onBeforeRequest((details, callback) => callback({ cancel: details.url !== renderUrl && !details.url.startsWith("data:") }));
  // An in-memory, exact private-session route avoids Chromium's data-navigation
  // URL limit for a full CJK font. No filesystem or network route is available.
  await renderSession.protocol.handle("https", request => request.url === renderUrl
    ? new Response(html, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" } }) : new Response("Blocked", { status: 403 }));
  let window;
  const abort = () => { if (window && !window.isDestroyed()) window.destroy(); };
  const timeout = setTimeout(abort, 30_000);
  try {
    signal?.throwIfAborted();
    window = new BrowserWindow({ show: false, width, height, useContentSize: true,
      frame: false, transparent: subtitle, backgroundColor: subtitle ? "#00000000" : "#ffffff", webPreferences: { session: renderSession, sandbox: true,
        contextIsolation: true, nodeIntegration: false, webSecurity: true, javascript: false } });
    signal?.addEventListener("abort", abort, { once: true });
    window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
    window.webContents.on("will-navigate", event => event.preventDefault());
    await window.loadURL(renderUrl);
    signal?.throwIfAborted();
    if (format === "pdf") return await window.webContents.printToPDF({ printBackground: true, preferCSSPageSize: true });
    let fontEvidence;
    if (subtitle) {
      const debuggerAdapter = window.webContents.debugger;
      debuggerAdapter.attach("1.3");
      await debuggerAdapter.sendCommand("DOM.enable"); await debuggerAdapter.sendCommand("CSS.enable");
      const root = await debuggerAdapter.sendCommand("DOM.getDocument");
      const deadline = Date.now() + 15_000; fontEvidence = [];
      for (const id of subtitleNodeIds) {
        signal?.throwIfAborted();
        const { nodeId } = await debuggerAdapter.sendCommand("DOM.querySelector", { nodeId: root.root.nodeId, selector: `#${id}` });
        if (!nodeId) throw Error("字幕布局节点缺失");
        let fonts;
        while (Date.now() < deadline) {
          signal?.throwIfAborted();
          ({ fonts } = await debuggerAdapter.sendCommand("CSS.getPlatformFontsForNode", { nodeId }));
          if (fonts?.length && fonts.every(font => font.isCustomFont && font.glyphCount > 0)) break;
          await new Promise(resolve => setTimeout(resolve, 30));
        }
        if (!fonts?.length || fonts.some(font => !font.isCustomFont || !Number.isInteger(font.glyphCount) || font.glyphCount <= 0)) throw Error("字幕固定字体未被实际使用，不能使用系统字体替代；作品原文已保留");
        const { model } = await debuggerAdapter.sendCommand("DOM.getBoxModel", { nodeId });
        const points = model?.border;
        if (!Array.isArray(points) || points.length !== 8 || points.some((value, index) => !Number.isFinite(value)
          || value < -.01 || value > (index % 2 ? height : width) + .01)) throw Error(`字幕 ${id} 超出画面安全区域，请拆分字幕或调整时间线；作品原文已保留`);
        fontEvidence.push({ nodeId: id, fonts: fonts.map(({ familyName, postScriptName, isCustomFont, glyphCount }) => ({ familyName, postScriptName, isCustomFont, glyphCount })) });
      }
      debuggerAdapter.detach();
    }
    signal?.throwIfAborted();
    const image = await window.webContents.capturePage({ x: 0, y: 0, width, height }, { stayHidden: true });
    // Normalize display scaling so exported pixels match document coordinates.
    const normalized = image.resize({ width, height });
    if (subtitle) {
      const bitmap = normalized.toBitmap(); let transparentPixels = 0, visiblePixels = 0;
      for (let index = 3; index < bitmap.length; index += 4) { if (bitmap[index] === 0) transparentPixels++; else visiblePixels++; }
      if (!transparentPixels || !visiblePixels) throw Error("字幕栅格没有有效透明背景或可见字形");
      return { bytes: normalized.toPNG(), fontEvidence };
    }
    return normalized.toPNG();
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", abort);
    if (window && !window.isDestroyed()) window.destroy();
    renderSession.protocol.unhandle("https");
    await renderSession.clearStorageData();
    await renderSession.clearCache();
  }
}
