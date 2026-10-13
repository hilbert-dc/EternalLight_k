// @ts-check
const MAX_FONT_BYTES = 32 * 1024 * 1024;
const fail = message => { throw Error(message); };

function bounded(buffer, offset, length, start = 0, end = buffer.length) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < start || length < 0 || offset + length > end) fail("字幕字体表越界或损坏");
}
/** A glyph coverage check prevents .notdef/system fallback from being accepted.
 * Chromium remains the shaping and rendering authority; only Unicode cmap 4/12
 * of the explicitly selected first face is accepted, with strict bounds. */
export function subtitleFontCoverage(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 12 || bytes.length > MAX_FONT_BYTES) fail("字幕字体为空、损坏或超过 32 MB");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const buffer = { length: bytes.length, readUInt16BE: offset => view.getUint16(offset), readInt16BE: offset => view.getInt16(offset),
    readUInt32BE: offset => view.getUint32(offset), toString: (_encoding, start, end) => String.fromCharCode(...bytes.subarray(start, end)) };
  let face = 0;
  if (buffer.toString("ascii", 0, 4) === "ttcf") {
    bounded(buffer, 0, 16); const count = buffer.readUInt32BE(8);
    if (!count || count > 32) fail("字幕字体集合无效");
    bounded(buffer, 12, count * 4); face = buffer.readUInt32BE(12);
  }
  bounded(buffer, face, 12);
  const signature = buffer.readUInt32BE(face);
  if (![0x00010000, 0x4f54544f].includes(signature)) fail("字幕需要有效的 TrueType 或 OpenType 字体");
  const count = buffer.readUInt16BE(face + 4);
  if (!count || count > 256) fail("字幕字体表数量无效");
  bounded(buffer, face + 12, count * 16);
  const tables = new Map();
  for (let index = 0; index < count; index++) {
    const offset = face + 12 + index * 16, tag = buffer.toString("ascii", offset, offset + 4);
    if (tables.has(tag)) fail("字幕字体表重复");
    const start = buffer.readUInt32BE(offset + 8), length = buffer.readUInt32BE(offset + 12);
    bounded(buffer, start, length); tables.set(tag, { start, end: start + length });
  }
  const cmap = tables.get("cmap"), maxp = tables.get("maxp");
  if (!cmap || !maxp) fail("字幕字体缺少 Unicode 字形表");
  bounded(buffer, maxp.start, 6, maxp.start, maxp.end); const glyphCount = buffer.readUInt16BE(maxp.start + 4);
  bounded(buffer, cmap.start, 4, cmap.start, cmap.end);
  if (buffer.readUInt16BE(cmap.start) !== 0) fail("字幕字体 cmap 版本无效");
  const encodings = buffer.readUInt16BE(cmap.start + 2);
  if (!encodings || encodings > 64) fail("字幕字体 cmap 数量无效");
  bounded(buffer, cmap.start + 4, encodings * 8, cmap.start, cmap.end);
  const mappings = [], seen = new Set();
  for (let index = 0; index < encodings; index++) {
    const record = cmap.start + 4 + index * 8, platform = buffer.readUInt16BE(record), encoding = buffer.readUInt16BE(record + 2);
    if (platform !== 0 && !(platform === 3 && [1, 10].includes(encoding))) continue;
    const start = cmap.start + buffer.readUInt32BE(record + 4);
    if (seen.has(start)) continue; seen.add(start);
    bounded(buffer, start, 4, cmap.start, cmap.end); const format = buffer.readUInt16BE(start);
    if (format === 12) {
      bounded(buffer, start, 16, cmap.start, cmap.end);
      const length = buffer.readUInt32BE(start + 4), groups = buffer.readUInt32BE(start + 12), end = start + length;
      if (length < 16 || groups > 65_536) fail("字幕字体 Unicode 分组无效");
      bounded(buffer, start, length, cmap.start, cmap.end); bounded(buffer, start + 16, groups * 12, start, end);
      const ranges = []; let previous = -1;
      for (let group = 0; group < groups; group++) {
        const offset = start + 16 + group * 12, first = buffer.readUInt32BE(offset), last = buffer.readUInt32BE(offset + 4), glyph = buffer.readUInt32BE(offset + 8);
        if (first <= previous || last < first || last > 0x10ffff || glyph + last - first >= glyphCount) fail("字幕字体 Unicode 分组越界");
        ranges.push({ first, last, glyph }); previous = last;
      }
      mappings.push(code => { let low = 0, high = ranges.length - 1; while (low <= high) { const mid = (low + high) >>> 1, range = ranges[mid]; if (code < range.first) high = mid - 1; else if (code > range.last) low = mid + 1; else return range.glyph + code - range.first; } return 0; });
    } else if (format === 4) {
      const length = buffer.readUInt16BE(start + 2), end = start + length;
      bounded(buffer, start, length, cmap.start, cmap.end); bounded(buffer, start, 16, start, end);
      const segments = buffer.readUInt16BE(start + 6) / 2;
      if (!Number.isInteger(segments) || !segments || segments > 8192 || length < 16 + segments * 8) fail("字幕字体 Unicode 段无效");
      const ends = start + 14, starts = ends + segments * 2 + 2, deltas = starts + segments * 2, offsets = deltas + segments * 2;
      const ranges = []; let previous = -1;
      for (let segment = 0; segment < segments; segment++) {
        const first = buffer.readUInt16BE(starts + segment * 2), last = buffer.readUInt16BE(ends + segment * 2), delta = buffer.readInt16BE(deltas + segment * 2), rangeOffset = buffer.readUInt16BE(offsets + segment * 2);
        if (first <= previous || last < first) fail("字幕字体 Unicode 段顺序无效");
        if (rangeOffset % 2) fail("字幕字体 Unicode 字形偏移无效");
        if (rangeOffset) bounded(buffer, offsets + segment * 2 + rangeOffset, (last - first + 1) * 2, start, end);
        ranges.push({ first, last, delta, rangeOffset, base: offsets + segment * 2 }); previous = last;
      }
      mappings.push(code => { if (code > 0xffff) return 0; let low = 0, high = ranges.length - 1; while (low <= high) { const mid = (low + high) >>> 1, range = ranges[mid]; if (code < range.first) high = mid - 1; else if (code > range.last) low = mid + 1; else { const raw = range.rangeOffset ? buffer.readUInt16BE(range.base + range.rangeOffset + (code - range.first) * 2) : code; return range.rangeOffset && !raw ? 0 : (raw + range.delta) & 0xffff; } } return 0; });
    }
  }
  if (!mappings.length || !glyphCount) fail("字幕字体没有支持的 Unicode 字形映射");
  return { mimeType: signature === 0x4f54544f ? "font/otf" : "font/ttf",
    covers: code => mappings.some(mapping => { const glyph = mapping(code); return glyph > 0 && glyph < glyphCount; }) };
}

export function subtitleCaptionError(caption, font) {
  if (typeof caption !== "string") return "字幕文本无效";
  const characters = [...caption];
  if (characters.length > 20_000 || new TextEncoder().encode(caption).byteLength > 128 * 1024) return "字幕文本过长";
  for (const character of characters) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff || code < 32 && ![9, 10, 13].includes(code) || code >= 0x7f && code <= 0x9f) return "字幕包含无效字符";
    if (/\s/u.test(character) || [0x200c, 0x200d].includes(code) || code >= 0xfe00 && code <= 0xfe0f || code >= 0xe0100 && code <= 0xe01ef) continue;
    if (!font.covers(code)) return `字幕字体缺少字形 U+${code.toString(16).toUpperCase()}，请配置覆盖该字符的字体`;
  }
  return null;
}
