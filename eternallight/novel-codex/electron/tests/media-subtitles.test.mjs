import assert from "node:assert/strict";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import { inspectSubtitleFont, prepareSubtitlePlan, subtitleFrameRange } from "../media-subtitles.mjs";

const time = (value, numerator = 1000, denominator = 1) => ({ value, rate: { numerator, denominator } });
function fontFixture() {
  const groups = [[32, 126, 1], [0x4e2d, 0x4e2d, 96], [0x6587, 0x6587, 97], [0x1f600, 0x1f600, 98]];
  const cmapBytes = 28 + groups.length * 12, buffer = Buffer.alloc(50 + cmapBytes);
  buffer.writeUInt32BE(0x00010000); buffer.writeUInt16BE(2, 4);
  buffer.write("maxp", 12); buffer.writeUInt32BE(44, 20); buffer.writeUInt32BE(6, 24); buffer.writeUInt16BE(99, 48);
  buffer.write("cmap", 28); buffer.writeUInt32BE(50, 36); buffer.writeUInt32BE(cmapBytes, 40);
  buffer.writeUInt16BE(1, 52); buffer.writeUInt16BE(3, 54); buffer.writeUInt16BE(10, 56); buffer.writeUInt32BE(12, 58);
  buffer.writeUInt16BE(12, 62); buffer.writeUInt32BE(cmapBytes - 12, 66); buffer.writeUInt32BE(groups.length, 74);
  groups.forEach((values, index) => values.forEach((value, field) => buffer.writeUInt32BE(value, 78 + index * 12 + field * 4)));
  return buffer;
}
const clip = (id, caption, start, duration) => ({ id, caption, start: time(start), duration: time(duration) });
const document = tracks => ({ kind: "animation", width: 320, height: 180, frameRate: { numerator: 24, denominator: 1 }, timeline: { tracks } });

test("half-open subtitle frames use exact rational arithmetic, including different time bases and fractional frame rates", () => {
  assert.deepEqual(subtitleFrameRange(time(250), time(500), { numerator: 24, denominator: 1 }), { startFrame: 6, endFrame: 18 });
  assert.deepEqual(subtitleFrameRange(time(260), time(500), { numerator: 24, denominator: 1 }), { startFrame: 7, endFrame: 19 });
  assert.deepEqual(subtitleFrameRange(time(1, 3), time(2, 7), { numerator: 30000, denominator: 1001 }), { startFrame: 10, endFrame: 19 });
  assert.throws(() => subtitleFrameRange(time(1), time(1), { numerator: 24, denominator: 1 }), /输出帧/);
  assert.throws(() => subtitleFrameRange(time(Number.MAX_SAFE_INTEGER), time(1), { numerator: 24, denominator: 1 }), /超限/);
});

test("fixed-font Unicode coverage rejects absent glyphs and malformed cmap boundaries", () => {
  const bytes = fontFixture(), font = inspectSubtitleFont(bytes);
  assert.equal(font.covers(0x4e2d), true); assert.equal(font.covers(0x1f600), true); assert.equal(font.covers(0x4e00), false);
  assert.equal(font.covers(0x1f), false); assert.equal(font.covers(0x7f), false);
  const broken = Buffer.from(bytes); broken.writeUInt32BE(bytes.length, 36);
  assert.throws(() => inspectSubtitleFont(broken), /越界/);
  const overlap = Buffer.from(bytes); overlap.writeUInt32BE(64, 90);
  assert.throws(() => inspectSubtitleFont(overlap), /分组越界/);
});

test("format 4 glyph offsets are bounded and unsupported cmap cannot claim coverage", () => {
  const buffer = Buffer.alloc(94); buffer.writeUInt32BE(0x00010000); buffer.writeUInt16BE(2, 4);
  buffer.write("maxp", 12); buffer.writeUInt32BE(44, 20); buffer.writeUInt32BE(6, 24); buffer.writeUInt16BE(2, 48);
  buffer.write("cmap", 28); buffer.writeUInt32BE(50, 36); buffer.writeUInt32BE(44, 40);
  buffer.writeUInt16BE(1, 52); buffer.writeUInt16BE(3, 54); buffer.writeUInt16BE(1, 56); buffer.writeUInt32BE(12, 58);
  buffer.writeUInt16BE(4, 62); buffer.writeUInt16BE(32, 64); buffer.writeUInt16BE(4, 68);
  buffer.writeUInt16BE(65, 76); buffer.writeUInt16BE(0xffff, 78); buffer.writeUInt16BE(65, 82); buffer.writeUInt16BE(0xffff, 84);
  buffer.writeInt16BE(-64, 86); buffer.writeInt16BE(1, 88);
  const font = inspectSubtitleFont(buffer); assert.equal(font.covers(65), true); assert.equal(font.covers(66), false); assert.equal(font.covers(0x1f600), false);
  const broken = Buffer.from(buffer); broken.writeUInt16BE(0xfffe, 90); assert.throws(() => inspectSubtitleFont(broken), /越界/);
  const unsupported = Buffer.from(buffer); unsupported.writeUInt16BE(13, 62); assert.throws(() => inspectSubtitleFont(unsupported), /没有支持/);
});

test("overlapping subtitles become stable ordered states, muted tracks vanish, and caption HTML is escaped", async t => {
  const directory = await mkdtemp(join(tmpdir(), "subtitle-plan-")); t.after(() => rm(directory, { recursive: true, force: true }));
  const fontPath = join(directory, "font.ttf"); await writeFile(fontPath, fontFixture());
  const doc = document([{ kind: "subtitle", muted: false, clips: [clip("one", "中文 A<script>", 250, 500), clip("two", "B", 500, 500)] },
    { kind: "subtitle", muted: true, clips: [clip("muted", "缺字", 0, 1000)] }]);
  const plan = await prepareSubtitlePlan({ document: doc, fontPath });
  assert.deepEqual(plan.states.map(({ startFrame, endFrame, clipIds }) => ({ startFrame, endFrame, clipIds })), [
    { startFrame: 6, endFrame: 12, clipIds: ["one"] }, { startFrame: 12, endFrame: 18, clipIds: ["one", "two"] }, { startFrame: 18, endFrame: 24, clipIds: ["two"] }]);
  assert.equal(plan.rasters.length, 3); assert.equal(plan.rasters[1].nodeIds.length, 2);
  assert.ok(plan.htmlForRaster(plan.rasters[1]).includes("background:transparent"));
  assert.ok(plan.htmlForRaster(plan.rasters[1]).includes("font-src data:"));
  assert.ok(plan.htmlForRaster(plan.rasters[0]).includes("&lt;script&gt;")); assert.ok(!plan.htmlForRaster(plan.rasters[0]).includes("<script>"));
  const blank = document([{ kind: "subtitle", clips: [clip("blank", " \n\t", 0, 1000)] }]), originalBlank = JSON.stringify(blank);
  const blankPlan = await prepareSubtitlePlan({ document: blank, fontPath: null });
  assert.deepEqual(blankPlan.states, []); assert.equal(blankPlan.fontDigest, null); assert.equal(JSON.stringify(blank), originalBlank);
  await assert.rejects(prepareSubtitlePlan({ document: document([{ kind: "subtitle", clips: [clip("missing", "缺", 0, 1000)] }]), fontPath }), /缺少字形/);
  await assert.rejects(prepareSubtitlePlan({ document: doc, fontPath: null }), /绝对路径/);
  const controller = new AbortController(); controller.abort(Error("cancelled"));
  await assert.rejects(prepareSubtitlePlan({ document: doc, fontPath, signal: controller.signal }), /cancelled/);
  await assert.rejects(prepareSubtitlePlan({ document: { ...doc, width: 8192, height: 8192, timeline: { tracks: [{ kind: "subtitle", clips: Array.from({ length: 5 }, (_, index) => clip(String(index), "A", index * 1000, 500)) }] } }, fontPath }), /栅格总量/);
});
