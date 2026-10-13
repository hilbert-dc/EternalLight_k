import { spawnSync } from "node:child_process";
import { availableParallelism } from "node:os";
import { closeSync, copyFileSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveBuildLayout } from "./build-layout.mjs";
import { acquireBuildLock, assertBuildDirectory, recoverBuildOutput, replaceBuildOutput, resetBuildDirectory, sha256Artifact, writeBuildReport } from "./build-artifacts.mjs";
import { verifyMediaRuntimeBundle } from "./media-runtime-bundle.mjs";

const lockPath = fileURLToPath(new URL("./media-runtime-sources.json", import.meta.url));
export function verifySource(path, source) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || sha256Artifact(path) !== source.sha256) throw Error(`源码摘要不匹配：${source.file}`);
  return path;
}
export function validateArchiveEntries(entries, directory) {
  const names = entries.trim().split(/\r?\n/);
  if (!names.length || names.some(name => !name.startsWith(`${directory}/`) || name.includes("\\") || name.split("/").some(part => part === ".."))) throw Error("源码归档路径越界");
}
export function buildMediaRuntime({ layout = resolveBuildLayout() } = {}) {
  if (layout.platform !== "darwin" || process.platform !== "darwin" || layout.architecture !== process.arch) throw Error("源码构建当前仅支持本机 macOS；其他目标须提供独立验证的媒体 bundle");
  const unlock = acquireBuildLock(join(layout.staging, "media-build.lock"));
  const work = join(layout.staging, "media-build"), cache = join(layout.staging, "media-sources");
  const output = layout.mediaRuntime, candidate = `${output}.building`, previous = `${output}.previous`;
  const reportPath = join(layout.staging, "media-build-report.json");
  const report = { status: "running", target: `${layout.platform}-${layout.architecture}`, startedAt: new Date().toISOString(), sourceLockSha256: sha256Artifact(lockPath), commands: [] };
  let log;
  try {
    recoverBuildOutput(output, previous);
    resetBuildDirectory(work); resetBuildDirectory(candidate);
    assertBuildDirectory(cache); mkdirSync(cache, { recursive: true });
    log = openSync(join(work, "build.log"), "w");
    const environment = { PATH: "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin", HOME: work, TMPDIR: work, LC_ALL: "C" };
    const run = (stage, command, args, cwd = work, extra = {}) => {
      report.stage = stage; report.commands.push({ command, args }); writeBuildReport(reportPath, report);
      process.stdout.write(`${stage}\n`);
      const result = spawnSync(command, args, { cwd, env: { ...environment, ...extra }, stdio: ["ignore", log, log] });
      if (result.error || result.status !== 0) throw Error(`${stage} 失败；查看 ${join(work, "build.log")}: ${result.error?.message ?? result.status}`);
    };
    const lock = JSON.parse(readFileSync(lockPath, "utf8"));
    for (const source of Object.values(lock.sources)) {
      const path = join(cache, source.file);
      if (!existsSync(path)) {
        const temporary = `${path}.download`;
        try {
          run(`download ${source.file}`, "/usr/bin/curl", ["--fail", "--location", "--proto", "=https", "--proto-redir", "=https", "--retry", "3", "--retry-all-errors", "--max-time", "300", source.url, "--output", temporary]);
          verifySource(temporary, source); renameSync(temporary, path);
        } finally { rmSync(temporary, { force: true }); }
      }
      verifySource(path, source);
      if (source.directory) {
        const listing = spawnSync("/usr/bin/tar", ["-tf", path], { encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
        if (listing.status !== 0) throw Error("无法读取源码归档");
        validateArchiveEntries(listing.stdout, source.directory);
        run(`extract ${source.file}`, "/usr/bin/tar", ["-xf", path, "-C", work]);
      }
    }
    const prefix = join(work, "prefix"), x264 = join(work, lock.sources.x264.directory), ffmpeg = join(work, lock.sources.ffmpeg.directory);
    const jobs = String(Math.min(availableParallelism(), 8));
    const x264Arguments = [`--prefix=${prefix}`, "--enable-static", "--enable-pic", "--disable-cli", "--disable-opencl"];
    run("configure x264", "./configure", x264Arguments, x264);
    run("build x264", "make", ["-j", jobs], x264);
    run("install x264", "make", ["install-lib-static"], x264);
    const configureArguments = ["--enable-gpl", "--enable-libx264", "--disable-autodetect", "--disable-shared", "--enable-static", "--enable-zlib", "--disable-network", "--disable-doc", "--disable-debug", "--disable-ffplay", "--pkg-config-flags=--static"];
    run("configure FFmpeg", "./configure", configureArguments, ffmpeg, { PKG_CONFIG_LIBDIR: join(prefix, "lib/pkgconfig"), PKG_CONFIG_PATH: "" });
    run("build FFmpeg", "make", ["-j", jobs, "ffmpeg", "ffprobe"], ffmpeg);
    mkdirSync(join(candidate, "licenses"));
    const files = {};
    const copy = (source, path, role, licenseFile) => {
      const destination = join(candidate, path); copyFileSync(source, destination);
      files[path] = { role, sha256: sha256Artifact(destination), ...(licenseFile ? { licenseFile } : {}) };
    };
    copy(join(ffmpeg, "COPYING.GPLv2"), "licenses/ffmpeg.txt", "license");
    copy(join(x264, "COPYING"), "licenses/x264.txt", "license");
    copy(join(cache, lock.sources.fontLicense.file), "licenses/font.txt", "license");
    copy(join(ffmpeg, "ffmpeg"), "ffmpeg", "ffmpeg", "licenses/ffmpeg.txt");
    copy(join(ffmpeg, "ffprobe"), "ffprobe", "ffprobe", "licenses/ffmpeg.txt");
    copy(join(cache, lock.sources.font.file), "subtitle-font.ttf", "font", "licenses/font.txt");
    const manifest = { product: "novel-codex-media-runtime", target: { platform: layout.platform, architecture: layout.architecture },
      build: { sourceUrl: lock.sources.ffmpeg.url, sourceSha256: lock.sources.ffmpeg.sha256, ffmpegVersion: lock.ffmpegVersion,
        configureArguments, licenseExpression: "GPL-2.0-or-later", sources: lock.sources, x264Arguments },
      entrypoints: { ffmpeg: "ffmpeg", ffprobe: "ffprobe", font: "subtitle-font.ttf" }, files };
    writeFileSync(join(candidate, "media-runtime-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    report.stage = "native-and-electron-verification"; writeBuildReport(reportPath, report);
    const verified = verifyMediaRuntimeBundle({ directory: candidate, platform: layout.platform, architecture: layout.architecture });
    report.evidence = verified.execution; report.manifestSha256 = verified.manifestSha256;
    replaceBuildOutput(candidate, output, previous);
    report.status = "passed"; report.stage = "complete"; report.output = output;
    return report;
  } catch (error) { report.status = "failed"; report.error = error.message; throw error; }
  finally { report.completedAt = new Date().toISOString(); writeBuildReport(reportPath, report); if (log !== undefined) closeSync(log); unlock(); }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.stdout.write(`${JSON.stringify(buildMediaRuntime(), null, 2)}\n`);
}
