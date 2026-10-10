import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import childProcess from "node:child_process";
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { basename, join, resolve } from "node:path";

const mediaExecutables = new Map();
export const fixtureMediaCalls = [];
const sha = bytes => createHash("sha256").update(bytes).digest("hex");

function nativeExecutable(platform, architecture) {
  if (platform === "darwin") {
    const dependency = Buffer.from("/usr/lib/libSystem.B.dylib\0"), commandSize = Math.ceil((12 + dependency.length) / 8) * 8;
    const bytes = Buffer.alloc(32 + commandSize); bytes.writeUInt32LE(0xfeedfacf); bytes.writeUInt32LE(architecture === "arm64" ? 0x0100000c : 0x01000007, 4);
    bytes.writeUInt32LE(2, 12); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(commandSize, 20);
    bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(commandSize, 36); bytes.writeUInt32LE(12, 40); dependency.copy(bytes, 44);
    return bytes;
  }
  if (platform === "win32") {
    const bytes = Buffer.alloc(1024), pe = 128, optional = pe + 24;
    bytes.write("MZ"); bytes.writeUInt32LE(pe, 0x3c); bytes.write("PE\0\0", pe); bytes.writeUInt16LE(architecture === "arm64" ? 0xaa64 : 0x8664, pe + 4);
    bytes.writeUInt16LE(1, pe + 6); bytes.writeUInt16LE(240, pe + 20); bytes.writeUInt16LE(2, pe + 22); bytes.writeUInt16LE(0x20b, optional);
    bytes.writeUInt32LE(16, optional + 108); bytes.writeUInt32LE(0x1000, optional + 120); bytes.writeUInt32LE(40, optional + 124);
    const section = optional + 240; bytes.writeUInt32LE(0x1000, section + 12); bytes.writeUInt32LE(512, section + 16); bytes.writeUInt32LE(512, section + 20);
    bytes.writeUInt32LE(0x1040, 524); bytes.write("KERNEL32.dll\0", 576); return bytes;
  }
  throw Error("Media release fixtures target current macOS/Windows products only");
}

/** Native-format structure for packaging validation only; smoke subprocess responses remain an explicit mock. */
export function writeFixtureMediaRuntime(directory, { platform = process.platform, architecture = process.arch } = {}) {
  mkdirSync(join(directory, "licenses"), { recursive: true });
  const suffix = platform === "win32" ? ".exe" : "", font = Buffer.alloc(32); font.writeUInt32BE(0x00010000);
  writeFileSync(join(directory, `ffmpeg${suffix}`), nativeExecutable(platform, architecture));
  writeFileSync(join(directory, `ffprobe${suffix}`), nativeExecutable(platform, architecture));
  writeFileSync(join(directory, "subtitle-font.ttf"), font);
  writeFileSync(join(directory, "licenses", "ffmpeg.txt"), "Controlled packaging fixture for FFmpeg GPL-3.0-or-later.");
  writeFileSync(join(directory, "licenses", "font.txt"), "Controlled packaging fixture for font OFL-1.1.");
  const files = { [`ffmpeg${suffix}`]: { role: "ffmpeg", licenseFile: "licenses/ffmpeg.txt" }, [`ffprobe${suffix}`]: { role: "ffprobe", licenseFile: "licenses/ffmpeg.txt" },
    "subtitle-font.ttf": { role: "font", licenseFile: "licenses/font.txt" }, "licenses/ffmpeg.txt": { role: "license" }, "licenses/font.txt": { role: "license" } };
  for (const [path, metadata] of Object.entries(files)) metadata.sha256 = sha(readFileSync(join(directory, path)));
  const manifest = { product: "novel-codex-media-runtime", target: { platform, architecture },
    build: { sourceUrl: "https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz", sourceSha256: "f".repeat(64), ffmpegVersion: "8.1.2", configureArguments: ["--enable-gpl", "--enable-libx264"], licenseExpression: "GPL-3.0-or-later" },
    entrypoints: { ffmpeg: `ffmpeg${suffix}`, ffprobe: `ffprobe${suffix}`, font: "subtitle-font.ttf" }, files };
  writeFileSync(join(directory, "media-runtime-manifest.json"), JSON.stringify(manifest));
  return directory;
}

export function registerFixtureMediaRuntime(directory, platform = process.platform) {
  const suffix = platform === "win32" ? ".exe" : "";
  for (const name of ["ffmpeg", "ffprobe"]) mediaExecutables.set(realpathSync(join(directory, `${name}${suffix}`)), name);
}

export function installFixtureMediaExecution(mock) {
  const previousSpawnSync = childProcess.spawnSync;
  const launcher = mock.method(childProcess, "spawnSync", (command, args = [], options) => {
    if (args.length === 1 && basename(args[0]) === "media-runtime-smoke.cjs") {
      const configuration = JSON.parse(options.input);
      assert.equal(options.env.PATH, "");
      assert.equal(options.env.ELECTRON_RUN_AS_NODE, undefined);
      assert.equal(mediaExecutables.get(realpathSync(configuration.ffmpegPath)), "ffmpeg");
      assert.equal(mediaExecutables.get(realpathSync(configuration.ffprobePath)), "ffprobe");
      assert.ok(existsSync(configuration.fontPath));
      assert.equal(configuration.directory, options.cwd);
      fixtureMediaCalls.push({ name: "electronSubtitleSmoke", args: [...args], path: resolve(command), environmentPath: options.env.PATH });
      const evidence = { encode: "h264", audio: "aac", subtitles: "electronBundledFont", probe: "verified", frames: "verified", environmentPath: "empty",
        electronVersion: "38.8.6", visiblePixels: [0, 300, 0], subtitleEvidence: [{ rasterId: "fixture-explicit-mock", fontDigest: sha(readFileSync(configuration.fontPath)),
          periods: [{ startFrame: 6, endFrame: 18, clipIds: ["caption"] }], fontEvidence: [{ nodeId: "subtitle-0", fonts: [{ familyName: "fixture", isCustomFont: true, glyphCount: 20 }] }] }] };
      return { status: 0, signal: null, stdout: `MEDIA_RUNTIME_SMOKE ${JSON.stringify(evidence)}\n`, stderr: "" };
    }
    let name;
    try { name = mediaExecutables.get(realpathSync(command)); } catch { /* Delegate non-media processes unchanged. */ }
    if (!name) return previousSpawnSync(command, args, options);
    assert.equal(options.env.PATH, "", "release media smoke must execute with empty PATH");
    assert.equal(options.env.DYLD_LIBRARY_PATH, undefined);
    fixtureMediaCalls.push({ name, args: [...args], path: resolve(command), environmentPath: options.env.PATH });
    const output = stdout => ({ status: 0, signal: null, stdout, stderr: "" });
    if (args.length === 1 && args[0] === "-version") return output(`${name} version 8.1.2 controlled-native-fixture\n`);
    if (name === "ffmpeg" && args.length === 1 && args[0] === "-buildconf") return output("configuration: --enable-gpl --enable-libx264\n");
    return { status: 64, signal: null, stdout: "", stderr: "Unsupported controlled media fixture invocation" };
  });
  syncBuiltinESMExports();
  return () => { launcher.mock.restore(); syncBuiltinESMExports(); mediaExecutables.clear(); fixtureMediaCalls.length = 0; };
}
