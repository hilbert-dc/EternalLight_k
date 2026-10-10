import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { AnimationPreview } from "../../../src/multimedia/AnimationPreview";
import "../../../src/styles.css";

const check = (value, message) => { if (!value) throw Error(message); };
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const until = async (predicate, message) => { for (let attempt = 0; attempt < 500 && !predicate(); attempt++) await tick(); check(predicate(), message); };
const time = value => ({ value, rate: { numerator: 1000, denominator: 1 } });
let updateCaption;
function Fixture() {
  const [caption, setCaption] = useState("中文 Subtitle\n第二行"); updateCaption = setCaption;
  const document = { kind: "animation" as const, title: "字幕", width: 320, height: 180, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [{ id: "subtitle", title: "字幕", kind: "subtitle" as const, muted: false, clips: [{ id: "one", shotId: null, assetRevisionId: null, start: time(0), duration: time(1000), sourceStart: time(0), playbackRate: { numerator: 1, denominator: 1 }, caption }] }] } };
  return <main style={{ width: 320 }}><AnimationPreview document={document} assets={[]} viewStorageKey="subtitle-preview-fixture" readSubtitleFont={window.readSubtitleFont} /></main>;
}
createRoot(document.getElementById("root")).render(<Fixture />);
(async () => { try {
  await until(() => document.querySelector(".mm-preview-caption")?.textContent.includes("第二行"), `Fixed font preview missing: ${document.body.textContent}`);
  check(document.querySelector('meta[http-equiv="Content-Security-Policy"]').getAttribute("content").includes("connect-src 'self' creative-media:"), "Production CSP not used");
  window.previewReady = true;
  await until(() => window.fontProofReceived, "Actual glyph font proof missing");
  updateCaption(String.fromCodePoint(0x10ffff));
  await until(() => document.querySelector(".mm-preview-font-status")?.textContent.includes("缺少字形"), "Missing glyph silently fell back");
  check(!document.querySelector(".mm-preview-caption"), "Missing glyph painted fallback text");
  updateCaption("中文\n".repeat(50));
  await until(() => document.querySelector(".mm-preview-font-status")?.textContent.includes("安全区域"), "Overflow silently clipped");
  check(document.querySelector<SVGElement>(".mm-subtitles-layer").style.visibility === "hidden", "Overflow remains visible");
  window.testResult = { passed: true, missingGlyphRejected: true, overflowRejected: true, csp: true };
} catch (reason) { window.testResult = { passed: false, error: String(reason.stack ?? reason), dom: document.body.textContent }; } })();
