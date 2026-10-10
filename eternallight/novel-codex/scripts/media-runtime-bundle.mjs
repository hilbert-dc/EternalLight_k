import { createHash, randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, isAbsolute, join, posix, relative, resolve } from "node:path";

const MANIFEST = "media-runtime-manifest.json";
const require = createRequire(import.meta.url);
const subtitleSmokePath = fileURLToPath(new URL("./media-runtime-smoke.cjs", import.meta.url));
const MAX_FILE_BYTES = 512 * 1024 * 1024;
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const validDigest = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const fail = message => { throw Error(`便携媒体运行时${message}`); };
const roles = new Set(["ffmpeg", "ffprobe", "library", "license", "font"]);

function safePath(value) {
  if (typeof value !== "string" || !value || value.length > 240 || !/^[A-Za-z0-9_.\/-]+$/.test(value)
    || value.split("/").some(part => !part || [".", ".."].includes(part) || part.endsWith(".") || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(part)) || isAbsolute(value)) fail("文件路径无效");
  return value;
}
function inside(root, path) { const result = relative(root, path); return result === "" || !result.startsWith("..") && !isAbsolute(result); }
function fileTree(directory) {
  const files = [];
  const visit = (path, prefix = "", depth = 0) => {
    if (depth > 8) fail("目录层级过深");
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const name = prefix ? `${prefix}/${entry.name}` : entry.name;
      safePath(name);
      if (entry.isDirectory()) visit(join(path, entry.name), name, depth + 1);
      else if (entry.isFile()) files.push(name);
      else fail("不能包含符号链接或特殊文件");
      if (files.length > 512) fail("文件数量过多");
    }
  };
  visit(directory);
  return files.sort();
}
function checkedFile(root, path) {
  const absolute = join(root, ...safePath(path).split("/"));
  const info = lstatSync(absolute);
  if (!info.isFile() || info.isSymbolicLink() || info.size <= 0 || info.size > MAX_FILE_BYTES || !inside(root, realpathSync(absolute))) fail(`文件无效：${path}`);
  return absolute;
}
function assertAbsent(path) {
  try { lstatSync(path); fail("发布媒体目录已存在，请使用新的构建资源目录"); }
  catch (error) { if (error.code !== "ENOENT") throw error; }
}
function boundedString(buffer, offset, end) {
  if (!Number.isInteger(offset) || offset < 0 || offset >= end || end > buffer.length) fail("本机程序中的字符串位置无效");
  const zero = buffer.indexOf(0, offset);
  if (zero < offset || zero >= end || zero - offset > 4096) fail("本机程序中的字符串无效");
  const value = buffer.toString("utf8", offset, zero);
  if (!value || /[\p{Cc}]/u.test(value)) fail("本机程序的依赖名称无效");
  return value;
}

function inspectMachO(bytes) {
  const slice = (start, size) => {
    if (size < 32 || start < 0 || start + size > bytes.length || bytes.readUInt32LE(start) !== 0xfeedfacf) fail("需要有效的 64 位 Mach-O 程序");
    const cpu = bytes.readUInt32LE(start + 4), architecture = cpu === 0x0100000c ? "arm64" : cpu === 0x01000007 ? "x64" : null;
    if (!architecture) fail("Mach-O 架构不在发布范围内");
    const type = bytes.readUInt32LE(start + 12), binaryKind = type === 2 ? "executable" : type === 6 ? "library" : null;
    if (!binaryKind) fail("Mach-O 须为可执行程序或动态库");
    const commands = bytes.readUInt32LE(start + 16), sizeOfCommands = bytes.readUInt32LE(start + 20);
    if (commands > 4096 || sizeOfCommands > size - 32) fail("Mach-O 载入命令无效");
    const dependencies = [], rpaths = [];
    let offset = start + 32;
    for (let index = 0; index < commands; index++) {
      if (offset + 8 > start + 32 + sizeOfCommands) fail("Mach-O 载入命令截断");
      const command = bytes.readUInt32LE(offset), commandSize = bytes.readUInt32LE(offset + 4);
      if (commandSize < 8 || offset + commandSize > start + 32 + sizeOfCommands) fail("Mach-O 载入命令越界");
      const plain = command & 0x7fffffff;
      if ([0xc, 0x18, 0x1f, 0x20, 0x23].includes(plain) || command === 0x8000001c) {
        if (commandSize < 12) fail("Mach-O 依赖命令截断");
        const value = boundedString(bytes, offset + bytes.readUInt32LE(offset + 8), offset + commandSize);
        if (command === 0x8000001c) rpaths.push(value); else dependencies.push(value);
      }
      offset += commandSize;
    }
    if (offset !== start + 32 + sizeOfCommands) fail("Mach-O 载入命令大小不一致");
    return { architecture, dependencies, rpaths, binaryKind };
  };
  if (bytes.length >= 32 && bytes.readUInt32LE(0) === 0xfeedfacf) return [slice(0, bytes.length)];
  if (bytes.length < 8 || ![0xcafebabe, 0xcafebabf].includes(bytes.readUInt32BE(0))) fail("程序不是 Mach-O；脚本和开发机器程序不能作为发布媒体组件");
  const wide = bytes.readUInt32BE(0) === 0xcafebabf, count = bytes.readUInt32BE(4), entryBytes = wide ? 32 : 20;
  if (count < 1 || count > 8 || 8 + count * entryBytes > bytes.length) fail("Universal Mach-O 表无效");
  const slices = [];
  for (let index = 0; index < count; index++) {
    const entry = 8 + index * entryBytes;
    const start = wide ? Number(bytes.readBigUInt64BE(entry + 8)) : bytes.readUInt32BE(entry + 8);
    const size = wide ? Number(bytes.readBigUInt64BE(entry + 16)) : bytes.readUInt32BE(entry + 12);
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(size) || start < 8 + count * entryBytes) fail("Universal Mach-O 切片无效");
    const parsed = slice(start, size);
    if (slices.some(previous => previous.architecture === parsed.architecture)) fail("Universal Mach-O 架构重复");
    slices.push(parsed);
  }
  return slices;
}

function inspectPE(bytes) {
  if (bytes.length < 64 || bytes.toString("ascii", 0, 2) !== "MZ") fail("程序不是 Windows PE；脚本不能作为发布媒体组件");
  const pe = bytes.readUInt32LE(0x3c);
  if (pe + 24 > bytes.length || bytes.toString("binary", pe, pe + 4) !== "PE\0\0") fail("PE 程序头无效");
  const machine = bytes.readUInt16LE(pe + 4), architecture = machine === 0x8664 ? "x64" : machine === 0xaa64 ? "arm64" : null;
  const sectionCount = bytes.readUInt16LE(pe + 6), optionalSize = bytes.readUInt16LE(pe + 20), optional = pe + 24;
  if (!architecture || sectionCount < 1 || sectionCount > 100 || optionalSize < 128 || optional + optionalSize > bytes.length || bytes.readUInt16LE(optional) !== 0x20b) fail("需要有效的 64 位 Windows 程序");
  const numberOfDirectories = bytes.readUInt32LE(optional + 108), sectionStart = optional + optionalSize;
  if (numberOfDirectories > 16 || 112 + numberOfDirectories * 8 > optionalSize || sectionStart + sectionCount * 40 > bytes.length) fail("PE 节或目录越界");
  const sections = [];
  for (let index = 0; index < sectionCount; index++) {
    const offset = sectionStart + index * 40;
    const section = { size: bytes.readUInt32LE(offset + 16), rva: bytes.readUInt32LE(offset + 12), offset: bytes.readUInt32LE(offset + 20) };
    if (section.offset + section.size > bytes.length) fail("PE 数据节截断");
    sections.push(section);
  }
  const rvaOffset = rva => {
    const section = sections.find(section => rva >= section.rva && rva < section.rva + section.size);
    if (!section) fail("PE 依赖地址越界");
    return section.offset + rva - section.rva;
  };
  const dependencies = [];
  const readImports = (directoryIndex, descriptorSize, nameOffset, delay) => {
    if (numberOfDirectories <= directoryIndex) return;
    const rva = bytes.readUInt32LE(optional + 112 + directoryIndex * 8), size = bytes.readUInt32LE(optional + 116 + directoryIndex * 8);
    if (!rva) { if (size) fail("PE 依赖目录无效"); return; }
    const start = rvaOffset(rva), end = Math.min(bytes.length, start + size);
    if (size < descriptorSize || size > 1024 * 1024 || start + size > bytes.length) fail("PE 依赖目录截断");
    for (let offset = start; offset + descriptorSize <= end; offset += descriptorSize) {
      if (bytes.subarray(offset, offset + descriptorSize).every(byte => byte === 0)) return;
      if (delay && bytes.readUInt32LE(offset) !== 1) fail("延迟依赖必须使用当前 RVA 格式");
      const name = boundedString(bytes, rvaOffset(bytes.readUInt32LE(offset + nameOffset)), bytes.length);
      if (!/^[A-Za-z0-9_.-]+\.dll$/i.test(name)) fail("Windows DLL 名称无效");
      dependencies.push(name);
    }
    fail("PE 依赖目录缺少终止记录");
  };
  readImports(1, 20, 12, false);
  readImports(13, 32, 4, true);
  const binaryKind = bytes.readUInt16LE(pe + 22) & 0x2000 ? "library" : "executable";
  return [{ architecture, dependencies, rpaths: [], binaryKind }];
}

export function inspectMediaNativeBinary(bytes, platform) {
  if (!(bytes instanceof Uint8Array)) fail("本机程序字节无效");
  const buffer = Buffer.from(bytes);
  if (platform === "darwin") return inspectMachO(buffer);
  if (platform === "win32") return inspectPE(buffer);
  fail("发布平台须为 macOS 或 Windows");
}

const windowsSystem = new Set(["kernel32.dll", "kernelbase.dll", "ntdll.dll", "user32.dll", "advapi32.dll", "shell32.dll", "shlwapi.dll", "ole32.dll", "oleaut32.dll", "uuid.dll", "gdi32.dll", "gdi32full.dll", "winmm.dll", "ws2_32.dll", "bcrypt.dll", "crypt32.dll", "secur32.dll", "version.dll", "d3d11.dll", "dxgi.dll", "dxva2.dll", "mf.dll", "mfplat.dll", "mfuuid.dll", "propsys.dll", "avrt.dll", "psapi.dll", "setupapi.dll", "comdlg32.dll", "comctl32.dll", "imm32.dll", "dwmapi.dll", "uxtheme.dll", "winspool.drv", "ucrtbase.dll", "msvcrt.dll"]);
function verifyDependencyClosure(root, manifest) {
  const nativeFiles = Object.entries(manifest.files).filter(([, file]) => ["ffmpeg", "ffprobe", "library"].includes(file.role));
  const inspected = new Map(nativeFiles.map(([path]) => [path, inspectMediaNativeBinary(readFileSync(checkedFile(root, path)), manifest.target.platform)]));
  for (const [path, slices] of inspected) {
    if (!slices.some(slice => slice.architecture === manifest.target.architecture)) fail(`架构不匹配：${path}`);
    const expectedKind = manifest.files[path].role === "library" ? "library" : "executable";
    if (slices.some(slice => slice.binaryKind !== expectedKind)) fail(`本机程序类型不匹配：${path}`);
  }
  const executableDirectory = dirname(join(root, manifest.entrypoints.ffmpeg));
  const expand = (value, loaderDirectory) => {
    if (value === "@loader_path" || value.startsWith("@loader_path/")) return resolve(loaderDirectory, value.slice("@loader_path".length).replace(/^\//, ""));
    if (value === "@executable_path" || value.startsWith("@executable_path/")) return resolve(executableDirectory, value.slice("@executable_path".length).replace(/^\//, ""));
    fail("依赖路径必须相对程序，不能依赖 Homebrew、开发目录或系统 PATH");
  };
  const rootRpaths = manifest.target.platform === "darwin" ? [manifest.entrypoints.ffmpeg, manifest.entrypoints.ffprobe].flatMap(path => {
    const directory = dirname(join(root, path));
    return inspected.get(path).filter(slice => slice.architecture === manifest.target.architecture).flatMap(slice => slice.rpaths.map(value => expand(value, directory)));
  }) : [];
  const reached = new Set(), queue = [manifest.entrypoints.ffmpeg, manifest.entrypoints.ffprobe];
  while (queue.length) {
    const path = queue.shift();
    if (reached.has(path)) continue;
    reached.add(path);
    const native = inspected.get(path), slice = native?.find(slice => slice.architecture === manifest.target.architecture);
    if (!slice) fail(`依赖程序架构不匹配：${path}`);
    const loader = dirname(join(root, path));
    const rpaths = [...rootRpaths, ...slice.rpaths.map(value => expand(value, loader))];
    if (rpaths.some(path => !inside(root, path))) fail("RPATH 指向运行时之外");
    for (const dependency of slice.dependencies) {
      let dependencyPath;
      if (manifest.target.platform === "darwin") {
        if (posix.normalize(dependency) === dependency && /^\/(usr\/lib|System\/Library)\//.test(dependency)) continue;
        if (dependency.startsWith("@rpath/")) {
          const candidates = [...new Set(rpaths.map(path => resolve(path, dependency.slice(7))).filter(path => inside(root, path) && existsSync(path)))];
          if (candidates.length !== 1) fail(`RPATH 依赖缺失或歧义：${dependency}`);
          dependencyPath = candidates[0];
        } else dependencyPath = expand(dependency, loader);
      } else {
        const lower = dependency.toLowerCase();
        if (windowsSystem.has(lower) || /^(api-ms-win-|ext-ms-win-)[a-z0-9_.-]+\.dll$/.test(lower)) continue;
        const candidates = nativeFiles.map(([path]) => join(root, path)).filter(path => dirname(path) === loader && path.toLowerCase().endsWith(`/${lower}`.replaceAll("/", process.platform === "win32" ? "\\" : "/")));
        if (candidates.length !== 1) fail(`Windows DLL 未与程序一起交付：${dependency}`);
        dependencyPath = candidates[0];
      }
      if (!inside(root, dependencyPath)) fail(`依赖越界：${dependency}`);
      const key = relative(root, dependencyPath).replaceAll("\\", "/");
      if (manifest.files[key]?.role !== "library" || !inspected.has(key)) fail(`依赖未声明或缺失：${dependency}`);
      queue.push(key);
    }
  }
  if (nativeFiles.some(([path]) => !reached.has(path))) fail("包含未被程序引用的本机动态库");
  return { nativeFiles: nativeFiles.map(([path]) => path), architecture: manifest.target.architecture };
}

function validateManifest(root, manifest, platform, architecture) {
  if (manifest?.product !== "novel-codex-media-runtime" || manifest.target?.platform !== platform || manifest.target?.architecture !== architecture
    || !["darwin", "win32"].includes(platform) || !["arm64", "x64"].includes(architecture) || !manifest.files || Array.isArray(manifest.files)
    || Object.keys(manifest.files).length < 5 || Object.keys(manifest.files).length > 512) fail("清单或发布目标无效");
  const source = new URL(manifest.build?.sourceUrl);
  if (source.protocol !== "https:" || source.username || source.password || source.search || source.hash
    || !validDigest(manifest.build.sourceSha256) || typeof manifest.build.ffmpegVersion !== "string" || !/^[A-Za-z0-9_.-]{1,80}$/.test(manifest.build.ffmpegVersion)
    || !Array.isArray(manifest.build.configureArguments) || !manifest.build.configureArguments.every(value => typeof value === "string" && value.length <= 1000)
    || !manifest.build.configureArguments.includes("--enable-gpl") || !manifest.build.configureArguments.includes("--enable-libx264")
    || !["GPL-2.0-or-later", "GPL-3.0-or-later"].includes(manifest.build.licenseExpression)) fail("缺少固定构建来源、libx264 或许可证信息");
  for (const role of ["ffmpeg", "ffprobe", "font"]) {
    const path = safePath(manifest.entrypoints?.[role]);
    if (manifest.files[path]?.role !== role) fail(`缺少 ${role} 入口`);
  }
  const suffix = platform === "win32" ? ".exe" : "";
  if (manifest.entrypoints.ffmpeg !== `ffmpeg${suffix}` || manifest.entrypoints.ffprobe !== `ffprobe${suffix}` || manifest.entrypoints.font !== "subtitle-font.ttf") fail("程序和字幕字体入口必须固定在运行时根目录");
  for (const [path, file] of Object.entries(manifest.files)) {
    if (safePath(path) === MANIFEST || !roles.has(file.role) || !validDigest(file.sha256)
      || file.unsignedSha256 !== undefined && !validDigest(file.unsignedSha256)) fail("文件清单无效");
    const actual = checkedFile(root, path), bytes = readFileSync(actual);
    if (sha(bytes) !== file.sha256) fail(`文件摘要不匹配：${path}`);
    if (file.role !== "license" && (typeof file.licenseFile !== "string" || manifest.files[file.licenseFile]?.role !== "license")) fail(`缺少文件许可证：${path}`);
    if (file.role === "license" && (bytes.length > 5 * 1024 * 1024 || !bytes.toString("utf8").trim() || bytes.includes(0))) fail(`许可证文件无效：${path}`);
    if (file.role === "font" && !(bytes.length > 12 && ["OTTO", "ttcf"].includes(bytes.toString("ascii", 0, 4)) || bytes.length > 12 && bytes.readUInt32BE(0) === 0x00010000)) fail(`字体文件无效：${path}`);
  }
  const actual = fileTree(root).filter(path => path !== MANIFEST);
  if (JSON.stringify(actual) !== JSON.stringify(Object.keys(manifest.files).sort())) fail("文件集合与清单不一致");
  return verifyDependencyClosure(root, manifest);
}

function execute(path, args, options, label) {
  const result = spawnSync(path, args, { encoding: "utf8", timeout: 30_000, maxBuffer: 1024 * 1024, windowsHide: true, ...options });
  if (result.error || result.status !== 0) fail(`${label}失败：${String(result.error?.code ?? result.stderr ?? result.status).slice(-2000)}`);
  return result.stdout;
}

function smokeRuntime(root, manifest, electronExecutable) {
  if (manifest.target.platform !== process.platform || manifest.target.architecture !== process.arch) fail("实际媒体验收必须在相同目标系统执行");
  const directory = mkdtempSync(join(tmpdir(), "novel-media-runtime-smoke-"));
  const environment = Object.fromEntries(Object.entries({ PATH: "", SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR, TEMP: directory, TMP: directory, TMPDIR: directory }).filter(([, value]) => typeof value === "string"));
  const options = { cwd: directory, env: environment };
  try {
    const ffmpeg = join(root, manifest.entrypoints.ffmpeg), ffprobe = join(root, manifest.entrypoints.ffprobe);
    const versions = [execute(ffmpeg, ["-version"], options, "FFmpeg 启动"), execute(ffprobe, ["-version"], options, "FFprobe 启动")];
    for (let index = 0; index < versions.length; index++) if (!versions[index].startsWith(`${index ? "ffprobe" : "ffmpeg"} version ${manifest.build.ffmpegVersion} `)) fail("程序版本与固定构建来源不一致");
    const build = execute(ffmpeg, ["-buildconf"], options, "FFmpeg 构建配置");
    if (!["--enable-gpl", "--enable-libx264"].every(flag => build.includes(flag))) fail("实际程序缺少约定的 libx264/GPL 构建配置");
    const executable = electronExecutable ?? require("electron");
    if (!isAbsolute(executable ?? "") || !lstatSync(executable).isFile()) fail("字幕实测需要明确的同平台 Electron 程序");
    const output = execute(executable, [subtitleSmokePath], { ...options, timeout: 120_000,
      input: JSON.stringify({ directory, ffmpegPath: ffmpeg, ffprobePath: ffprobe, fontPath: join(root, manifest.entrypoints.font), platform: manifest.target.platform, architecture: manifest.target.architecture }) }, "Electron 字幕、H.264/AAC 与成片帧回读");
    const records = output.split(/\r?\n/).filter(line => line.startsWith("MEDIA_RUNTIME_SMOKE "));
    if (records.length !== 1) fail("缺少唯一实际字幕验收回执");
    const evidence = JSON.parse(records[0].slice("MEDIA_RUNTIME_SMOKE ".length));
    const fixedFontDigest = manifest.files[manifest.entrypoints.font].sha256;
    const fixedFontEvidence = Array.isArray(evidence.subtitleEvidence) && evidence.subtitleEvidence.length === 1 && evidence.subtitleEvidence.every(raster =>
      raster.fontDigest === fixedFontDigest && Array.isArray(raster.periods) && raster.periods.length === 1
      && raster.periods[0].startFrame === 6 && raster.periods[0].endFrame === 18
      && JSON.stringify(raster.periods[0].clipIds) === JSON.stringify(["caption"])
      && Array.isArray(raster.fontEvidence) && raster.fontEvidence.length > 0 && raster.fontEvidence.every(node =>
        typeof node.nodeId === "string" && Array.isArray(node.fonts) && node.fonts.length > 0
        && node.fonts.every(font => font.isCustomFont === true && Number.isSafeInteger(font.glyphCount) && font.glyphCount > 0)));
    if (evidence.encode !== "h264" || evidence.audio !== "aac" || evidence.subtitles !== "electronBundledFont" || evidence.probe !== "verified"
      || evidence.frames !== "verified" || evidence.environmentPath !== "empty" || !Array.isArray(evidence.visiblePixels)
      || evidence.visiblePixels.length !== 3 || evidence.visiblePixels[0] !== 0 || evidence.visiblePixels[1] < 100 || evidence.visiblePixels[2] !== 0
      || !fixedFontEvidence
      || evidence.electronVersion !== require("electron/package.json").version) fail("实际字幕渲染证据不完整或 Electron 版本与当前构建不一致");
    return evidence;
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

export function verifyMediaRuntimeBundle({ directory, platform = process.platform, architecture = process.arch, smoke = true, electronExecutable }) {
  if (!isAbsolute(directory ?? "")) fail("必须提供固定组件目录的绝对路径");
  const info = lstatSync(directory);
  if (!info.isDirectory() || info.isSymbolicLink()) fail("组件目录不能是符号链接");
  const root = realpathSync(directory), manifestPath = checkedFile(root, MANIFEST);
  if (lstatSync(manifestPath).size > 1024 * 1024) fail("清单过大");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const dependencies = validateManifest(root, manifest, platform, architecture);
  const execution = smoke ? smokeRuntime(root, manifest, electronExecutable) : null;
  return { manifest, manifestSha256: sha(readFileSync(manifestPath)), dependencies, execution,
    ffmpegPath: join(root, manifest.entrypoints.ffmpeg), ffprobePath: join(root, manifest.entrypoints.ffprobe), fontPath: join(root, manifest.entrypoints.font) };
}

/** Explicit fixed-source staging. No downloads, PATH lookup, implicit Homebrew copy, or partial empty bundle. */
export function stageMediaRuntimeBundle({ sourceDirectory, destinationDirectory, platform = process.platform, architecture = process.arch, nestedSigner = null }) {
  if (!isAbsolute(sourceDirectory ?? "") || !isAbsolute(destinationDirectory ?? "")) fail("需要明确的来源与发布目录");
  const verified = verifyMediaRuntimeBundle({ directory: sourceDirectory, platform, architecture, smoke: false });
  assertAbsent(destinationDirectory);
  mkdirSync(dirname(destinationDirectory), { recursive: true });
  const staging = `${resolve(destinationDirectory)}.${randomUUID()}.tmp`;
  mkdirSync(staging, { mode: 0o700 });
  try {
    const manifest = structuredClone(verified.manifest);
    for (const [path, file] of Object.entries(manifest.files)) {
      const target = join(staging, ...path.split("/"));
      mkdirSync(dirname(target), { recursive: true, mode: 0o700 });
      copyFileSync(checkedFile(realpathSync(sourceDirectory), path), target);
      if (["ffmpeg", "ffprobe", "library"].includes(file.role)) {
        if (platform !== "win32") chmodSync(target, 0o755);
        file.unsignedSha256 = file.sha256;
        nestedSigner?.(target);
        file.sha256 = sha(readFileSync(target));
      }
    }
    writeFileSync(join(staging, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
    verifyMediaRuntimeBundle({ directory: staging, platform, architecture, smoke: false });
    assertAbsent(destinationDirectory);
    renameSync(staging, destinationDirectory);
    return verifyMediaRuntimeBundle({ directory: destinationDirectory, platform, architecture, smoke: false });
  } finally { rmSync(staging, { recursive: true, force: true }); }
}
