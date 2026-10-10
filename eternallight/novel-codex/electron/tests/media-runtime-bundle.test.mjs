import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import childProcess from "node:child_process";
import { syncBuiltinESMExports } from "node:module";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { inspectMediaNativeBinary, stageMediaRuntimeBundle, verifyMediaRuntimeBundle } from "../../scripts/media-runtime-bundle.mjs";

const sha = value => createHash("sha256").update(value).digest("hex");
function macho(dependencies = ["/usr/lib/libSystem.B.dylib"], rpaths = [], architecture = "arm64", library = false) {
  const commands = [...dependencies.map(value => [0xc, value]), ...rpaths.map(value => [0x8000001c, value])].map(([command, value]) => {
    const string = Buffer.from(`${value}\0`), bytes = Buffer.alloc(Math.ceil((12 + string.length) / 8) * 8);
    bytes.writeUInt32LE(command, 0); bytes.writeUInt32LE(bytes.length, 4); bytes.writeUInt32LE(12, 8); string.copy(bytes, 12); return bytes;
  });
  const header = Buffer.alloc(32); header.writeUInt32LE(0xfeedfacf); header.writeUInt32LE(architecture === "arm64" ? 0x0100000c : 0x01000007, 4);
  header.writeUInt32LE(library ? 6 : 2, 12);
  header.writeUInt32LE(commands.length, 16); header.writeUInt32LE(commands.reduce((total, command) => total + command.length, 0), 20);
  return Buffer.concat([header, ...commands]);
}
function pe(dependencies = ["KERNEL32.dll"], architecture = "x64", library = false) {
  const bytes = Buffer.alloc(1536), header = 128, optional = header + 24;
  bytes.write("MZ"); bytes.writeUInt32LE(header, 0x3c); bytes.write("PE\0\0", header);
  bytes.writeUInt16LE(architecture === "arm64" ? 0xaa64 : 0x8664, header + 4); bytes.writeUInt16LE(1, header + 6); bytes.writeUInt16LE(240, header + 20);
  bytes.writeUInt16LE(library ? 0x2002 : 2, header + 22);
  bytes.writeUInt16LE(0x20b, optional); bytes.writeUInt32LE(16, optional + 108);
  bytes.writeUInt32LE(0x1000, optional + 120); bytes.writeUInt32LE((dependencies.length + 1) * 20, optional + 124);
  const section = optional + 240; bytes.writeUInt32LE(0x1000, section + 12); bytes.writeUInt32LE(1024, section + 16); bytes.writeUInt32LE(512, section + 20);
  let nameOffset = 512 + (dependencies.length + 1) * 20;
  for (let index = 0; index < dependencies.length; index++) {
    bytes.writeUInt32LE(0x1000 + nameOffset - 512, 512 + index * 20 + 12); bytes.write(`${dependencies[index]}\0`, nameOffset); nameOffset += dependencies[index].length + 1;
  }
  return bytes;
}
function sourceFixture(t, { platform = "darwin", architecture = platform === "darwin" ? "arm64" : "x64", dependencies, rpaths, library } = {}) {
  const directory = mkdtempSync(join(tmpdir(), "media-runtime-bundle-")); t.after(() => rmSync(directory, { recursive: true, force: true }));
  const source = join(directory, "source"); mkdirSync(source); mkdirSync(join(source, "licenses"));
  const suffix = platform === "win32" ? ".exe" : "", native = platform === "darwin" ? macho : pe;
  writeFileSync(join(source, `ffmpeg${suffix}`), platform === "darwin" ? native(dependencies, rpaths, architecture) : native(dependencies, architecture));
  writeFileSync(join(source, `ffprobe${suffix}`), platform === "darwin" ? native(undefined, undefined, architecture) : native(undefined, architecture));
  writeFileSync(join(source, "licenses", "ffmpeg.txt"), "FFmpeg GPL-3.0-or-later and accompanying libx264 sources.");
  writeFileSync(join(source, "licenses", "font.txt"), "OFL-1.1 font license and attribution.");
  const font = Buffer.alloc(32); font.writeUInt32BE(0x00010000); writeFileSync(join(source, "subtitle-font.ttf"), font);
  const files = { [`ffmpeg${suffix}`]: { role: "ffmpeg", licenseFile: "licenses/ffmpeg.txt" }, [`ffprobe${suffix}`]: { role: "ffprobe", licenseFile: "licenses/ffmpeg.txt" },
    "subtitle-font.ttf": { role: "font", licenseFile: "licenses/font.txt" }, "licenses/ffmpeg.txt": { role: "license" }, "licenses/font.txt": { role: "license" } };
  if (library) {
    mkdirSync(join(source, library.directory ?? "lib"), { recursive: true });
    const path = library.directory === "" ? library.name : `${library.directory ?? "lib"}/${library.name}`;
    writeFileSync(join(source, path), platform === "darwin" ? native(library.dependencies, library.rpaths, architecture, true) : native(library.dependencies, architecture, true));
    files[path] = { role: "library", licenseFile: "licenses/ffmpeg.txt" };
  }
  for (const [path, file] of Object.entries(files)) file.sha256 = sha(readFileSync(join(source, path)));
  const manifest = { product: "novel-codex-media-runtime", target: { platform, architecture }, build: { sourceUrl: "https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz", sourceSha256: "e".repeat(64), ffmpegVersion: "8.1.2", configureArguments: ["--enable-gpl", "--enable-libx264"], licenseExpression: "GPL-3.0-or-later" },
    entrypoints: { ffmpeg: `ffmpeg${suffix}`, ffprobe: `ffprobe${suffix}`, font: "subtitle-font.ttf" }, files };
  writeFileSync(join(source, "media-runtime-manifest.json"), JSON.stringify(manifest));
  return { directory, source, manifest, platform, architecture, update: () => writeFileSync(join(source, "media-runtime-manifest.json"), JSON.stringify(manifest)) };
}

test("fixed native runtime stages dependency closure and retains unsigned identity after signing", t => {
  const input = sourceFixture(t, { dependencies: ["/usr/lib/libSystem.B.dylib", "@rpath/libcodec.dylib"], rpaths: ["@loader_path/lib"], library: { name: "libcodec.dylib" } });
  const destinationDirectory = join(input.directory, "staged");
  const staged = stageMediaRuntimeBundle({ sourceDirectory: input.source, destinationDirectory, platform: input.platform, architecture: input.architecture,
    nestedSigner: path => appendFileSync(path, Buffer.from("signed")) });
  assert.equal(staged.dependencies.nativeFiles.length, 3);
  assert.notEqual(staged.manifest.files.ffmpeg.sha256, staged.manifest.files.ffmpeg.unsignedSha256);
  assert.equal(staged.manifest.files.ffmpeg.unsignedSha256, input.manifest.files.ffmpeg.sha256);
  assert.equal(staged.execution, null, "static test fixtures are not claimed as executable media verification");
  assert.throws(() => stageMediaRuntimeBundle({ sourceDirectory: input.source, destinationDirectory, platform: input.platform, architecture: input.architecture }), /目录已存在/);
});

test("external Homebrew dependency and escaping rpath block publication before files are staged", t => {
  const input = sourceFixture(t, { dependencies: ["/opt/homebrew/opt/x264/lib/libx264.164.dylib"] });
  const destinationDirectory = join(input.directory, "staged");
  assert.throws(() => stageMediaRuntimeBundle({ sourceDirectory: input.source, destinationDirectory, platform: input.platform, architecture: input.architecture }), /Homebrew/);
  assert.equal(existsSync(destinationDirectory), false);
  const encoded = macho(["@rpath/libcodec.dylib"], ["@loader_path/../../outside"]);
  writeFileSync(join(input.source, "ffmpeg"), encoded); input.manifest.files.ffmpeg.sha256 = sha(encoded); input.update();
  assert.throws(() => verifyMediaRuntimeBundle({ directory: input.source, platform: input.platform, architecture: input.architecture, smoke: false }), /RPATH 指向/);
});

test("digest, file-set, architecture, licence and symlink failures reject fixed bundles", t => {
  const input = sourceFixture(t);
  const options = { directory: input.source, platform: input.platform, architecture: input.architecture, smoke: false };
  const executable = join(input.source, "ffmpeg"), original = readFileSync(executable);
  appendFileSync(executable, "corrupt"); assert.throws(() => verifyMediaRuntimeBundle(options), /摘要不匹配/);
  writeFileSync(executable, original);
  writeFileSync(join(input.source, "unknown.txt"), "unknown"); assert.throws(() => verifyMediaRuntimeBundle(options), /文件集合/); rmSync(join(input.source, "unknown.txt"));
  assert.throws(() => verifyMediaRuntimeBundle({ ...options, architecture: "x64" }), /发布目标/);
  input.manifest.target.architecture = "x64"; input.update(); assert.throws(() => verifyMediaRuntimeBundle({ ...options, architecture: "x64" }), /架构不匹配/);
  input.manifest.target.architecture = "arm64"; delete input.manifest.files.ffmpeg.licenseFile; input.update(); assert.throws(() => verifyMediaRuntimeBundle(options), /许可证/);
  input.manifest.files.ffmpeg.licenseFile = "licenses/ffmpeg.txt"; input.update(); rmSync(executable); symlinkSync(join(input.source, "ffprobe"), executable);
  assert.throws(() => verifyMediaRuntimeBundle(options), /文件无效|符号链接/);
});

test("Windows PE imports require colocated declared DLLs and current target architecture", t => {
  const input = sourceFixture(t, { platform: "win32", dependencies: ["KERNEL32.dll", "vcruntime140.dll"], library: { directory: "", name: "vcruntime140.dll" } });
  const verified = verifyMediaRuntimeBundle({ directory: input.source, platform: "win32", architecture: "x64", smoke: false });
  assert.deepEqual(verified.dependencies.nativeFiles, ["ffmpeg.exe", "ffprobe.exe", "vcruntime140.dll"]);
  assert.throws(() => verifyMediaRuntimeBundle({ directory: input.source, platform: "win32", architecture: "x64", smoke: true }), /相同目标系统/);
  const fake = pe(["not-installed.dll"]); writeFileSync(join(input.source, "ffmpeg.exe"), fake); input.manifest.files["ffmpeg.exe"].sha256 = sha(fake); input.update();
  assert.throws(() => verifyMediaRuntimeBundle({ directory: input.source, platform: "win32", architecture: "x64", smoke: false }), /DLL 未/);
});

test("scripts, truncated native headers and malformed dependency offsets cannot impersonate media executables", () => {
  assert.throws(() => inspectMediaNativeBinary(Buffer.from("#!/bin/sh\nexit 0"), "darwin"), /不是 Mach-O/);
  assert.throws(() => inspectMediaNativeBinary(Buffer.from("MZ"), "win32"), /不是 Windows PE/);
  const malicious = macho(); malicious.writeUInt32LE(0xffffffff, 40);
  assert.throws(() => inspectMediaNativeBinary(malicious, "darwin"), /字符串位置/);
  const broken = pe(); broken.writeUInt32LE(0xffffffff, 512 + 12);
  assert.throws(() => inspectMediaNativeBinary(broken, "win32"), /地址越界/);
});

test("failing signer cleans its own temporary staging without deleting the original portable source", t => {
  const input = sourceFixture(t), destinationDirectory = join(input.directory, "failed");
  assert.throws(() => stageMediaRuntimeBundle({ sourceDirectory: input.source, destinationDirectory, platform: input.platform, architecture: input.architecture,
    nestedSigner: () => { throw Error("signing failed"); } }), /signing failed/);
  assert.deepEqual(readdirSync(input.directory), ["source"]);
  assert.ok(existsSync(join(input.source, "ffmpeg")));
});

test("publication requires the actual Electron subtitle and frame receipt after static checks", t => {
  const input = sourceFixture(t, { platform: process.platform, architecture: process.arch });
  let evidence = { encode: "h264", audio: "aac", subtitles: "electronBundledFont", probe: "verified", frames: "verified", environmentPath: "empty",
    electronVersion: "38.8.6", visiblePixels: [0, 300, 0], subtitleEvidence: [{ rasterId: "fixture-explicit-mock", fontDigest: input.manifest.files["subtitle-font.ttf"].sha256,
      periods: [{ startFrame: 6, endFrame: 18, clipIds: ["caption"] }], fontEvidence: [{ nodeId: "subtitle-0", fonts: [{ familyName: "fixture", isCustomFont: true, glyphCount: 20 }] }] }] };
  const calls = [];
  const launcher = t.mock.method(childProcess, "spawnSync", (command, args, options) => {
    assert.equal(options.env.PATH, ""); calls.push({ command, args });
    let stdout;
    if (args[0] === "-version") stdout = `${command.endsWith("ffprobe") || command.endsWith("ffprobe.exe") ? "ffprobe" : "ffmpeg"} version 8.1.2 mock\n`;
    else if (args[0] === "-buildconf") stdout = "configuration: --enable-gpl --enable-libx264\n";
    else {
      assert.ok(args[0].endsWith("media-runtime-smoke.cjs"));
      const request = JSON.parse(options.input);
      assert.equal(request.fontPath, realpathSync(join(input.source, "subtitle-font.ttf")));
      stdout = `MEDIA_RUNTIME_SMOKE ${JSON.stringify(evidence)}\n`;
    }
    return { status: 0, stdout, stderr: "" };
  });
  syncBuiltinESMExports();
  t.after(() => { launcher.mock.restore(); syncBuiltinESMExports(); });
  assert.equal(verifyMediaRuntimeBundle({ directory: input.source }).execution.frames, "verified");
  assert.equal(calls.length, 4);
  const raster = evidence.subtitleEvidence[0];
  for (const patch of [{ frames: "unverified" }, { subtitleEvidence: [] }, { visiblePixels: [0, 0, 0] }, { visiblePixels: [300, 300, 0] }, { electronVersion: "0.0.0" },
    { subtitleEvidence: [{ ...raster, fontDigest: "a".repeat(64) }] }, { subtitleEvidence: [{ ...raster, periods: [{ startFrame: 6, endFrame: 19, clipIds: ["caption"] }] }] },
    { subtitleEvidence: [{ ...raster, fontEvidence: [{ nodeId: "subtitle-0", fonts: [{ isCustomFont: false, glyphCount: 20 }] }] }] }]) {
    const original = evidence; evidence = { ...evidence, ...patch };
    assert.throws(() => verifyMediaRuntimeBundle({ directory: input.source }), /证据不完整/);
    evidence = original;
  }
  assert.equal(readdirSync(input.directory).filter(path => path !== "source").length, 0, "mock validation does not produce or claim an encoded result");
});
