// @ts-check
import { createHash, randomUUID } from "node:crypto";
import { mkdir, writeFile, link, unlink, lstat, realpath, open } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { basename, isAbsolute, join, relative } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execute = promisify(execFile);
const MAX_BYTES = 512 * 1024 * 1024;
function fail(message) { throw new Error(message); }
async function syncDirectory(path) {
  let handle;
  try {handle=await open(path,"r");await handle.sync();}
  catch(error){if(process.platform!=="win32"||!["EPERM","EISDIR","ENOTSUP"].includes(error.code))throw error;}
  finally {await handle?.close();}
}
export function mediaSignature(bytes) {
  if (bytes.subarray(0, 8).equals(Buffer.from([137,80,78,71,13,10,26,10]))) return { kind: "image", mimeType: "image/png" };
  if (bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255) return { kind: "image", mimeType: "image/jpeg" };
  if (["GIF87a", "GIF89a"].includes(bytes.toString("ascii", 0, 6))) return { kind: "image", mimeType: "image/gif" };
  const riff = bytes.toString("ascii", 0, 4) === "RIFF", type = bytes.toString("ascii", 8, 12);
  if (riff && type === "WEBP") return { kind: "image", mimeType: "image/webp" };
  if (riff && type === "WAVE") return { kind: "audio", mimeType: "audio/wav" };
  if (bytes.toString("ascii", 0, 4) === "fLaC") return { kind: "audio", mimeType: "audio/flac" };
  if (bytes.toString("ascii", 0, 4) === "OggS") return { kind: "audio", mimeType: "audio/ogg" };
  if (bytes[0] === 255 && (bytes[1] & 0xf6) === 0xf0) return { kind: "audio", mimeType: "audio/aac" };
  if (bytes.toString("ascii", 0, 3) === "ID3" || bytes[0] === 255 && (bytes[1] & 0xe0) === 0xe0) return { kind: "audio", mimeType: "audio/mpeg" };
  if (bytes.toString("ascii", 4, 8) === "ftyp") return { kind: "video", mimeType: "video/mp4" };
  if (bytes.subarray(0,4).equals(Buffer.from([0x1a,0x45,0xdf,0xa3]))) return { kind: "video", mimeType: "video/webm" };
  fail("不支持此媒体格式；请使用 PNG、JPEG、WebP、GIF、常见音频或 MP4/WebM");
}
export async function probeMediaFile(path, { ffprobePath = undefined, signal = undefined, processEnvironment = undefined } = {}) {
  if (!ffprobePath || !isAbsolute(ffprobePath)) fail("本机媒体校验组件尚未配置");
  const { stdout } = await execute(ffprobePath, ["-v", "error", "-protocol_whitelist", "file,pipe", "-show_streams", "-show_format", "-of", "json", path], { signal, env: processEnvironment, timeout: 30_000, maxBuffer: 1024 * 1024, windowsHide: true });
  const result = JSON.parse(stdout), streams = result.streams ?? [];
  const video = streams.find(stream => stream.codec_type === "video" && stream.disposition?.attached_pic !== 1);
  const audio = streams.find(stream => stream.codec_type === "audio");
  if (!video && !audio) fail("文件没有可解码的音视频轨道");
  const duration = Number(result.format?.duration ?? video?.duration ?? audio?.duration);
  if (!Number.isFinite(duration) || duration <= 0 || duration > 24 * 3600) fail("媒体时长无效或超过一天");
  if (video && (!Number.isInteger(video.width) || !Number.isInteger(video.height) || video.width <= 0 || video.height <= 0 || video.width * video.height > 100_000_000)) fail("视频尺寸无效");
  return { kind: video ? "video" : "audio", width: video?.width ?? null, height: video?.height ?? null,
    duration: { value: Math.round(duration * 1_000_000), rate: { numerator: 1_000_000, denominator: 1 } },
    sampleRate: audio ? Number(audio.sample_rate) || null : null, channels: audio?.channels ?? null };
}
export async function resolveMediaBlob({ rootDirectory, digest }) {
  if (!/^[a-f0-9]{64}$/.test(digest)) fail("素材摘要无效");
  const root = await realpath(rootDirectory), path = join(root, "blobs", digest.slice(0,2), digest);
  const info = await lstat(path), actual = await realpath(path);
  if (!info.isFile() || info.isSymbolicLink() || info.size > MAX_BYTES || relative(root, actual).startsWith("..")) fail("素材路径无效");
  const hash=createHash("sha256");let total=0;
  for await (const chunk of createReadStream(actual)){total+=chunk.length;if(total>MAX_BYTES)fail("素材完整性校验失败");hash.update(chunk);}
  if (hash.digest("hex") !== digest) fail("素材完整性校验失败");
  return actual;
}
export async function importMediaBytes({ bytes, name, rootDirectory, decodeImage, ffprobePath, signal = undefined }) {
  if (!(bytes instanceof Uint8Array) || !bytes.byteLength || bytes.byteLength > MAX_BYTES) fail("素材为空或超过 512 MB");
  if (typeof name !== "string" || basename(name) !== name || !name || name.length > 250 || /[\x00-\x1f]/.test(name)) fail("素材名称无效");
  signal?.throwIfAborted();
  const buffer = Buffer.from(bytes), signature = mediaSignature(buffer), digest = createHash("sha256").update(buffer).digest("hex");
  const relativePath = join("blobs", digest.slice(0,2), digest), path = join(rootDirectory, relativePath);
  await mkdir(join(rootDirectory, "blobs", digest.slice(0,2)), { recursive: true, mode: 0o700 });
  const root=await realpath(rootDirectory), parent=await realpath(join(rootDirectory,"blobs",digest.slice(0,2)));
  if(relative(root,parent).startsWith(".."))fail("素材目录不能位于受控存储之外");
  // Unique staging file allows full validation before publishing an immutable blob.
  const staging = `${path}.${randomUUID()}.tmp`;
  await writeFile(staging, buffer, { mode: 0o600, flag: "wx" });
  try {
    const file=await open(staging,"r");try{await file.sync();}finally{await file.close();}
    let dimensions;
    if (signature.kind === "image") {
      if (typeof decodeImage !== "function") fail("本机图片解码组件不可用");
      dimensions = await decodeImage(buffer);
      if (!dimensions || !Number.isInteger(dimensions.width) || !Number.isInteger(dimensions.height) || dimensions.width <= 0 || dimensions.height <= 0 || dimensions.width * dimensions.height > 100_000_000) fail("图片无法解码或尺寸过大");
      dimensions = { kind: "image", width: dimensions.width, height: dimensions.height, duration: null, sampleRate: null, channels: null };
    } else dimensions = await probeMediaFile(staging, { ffprobePath, signal });
    signal?.throwIfAborted();
    try { await link(staging, path); } catch (error) { if (error.code !== "EEXIST") throw error; await resolveMediaBlob({ rootDirectory, digest }); }
    await syncDirectory(parent);
    await syncDirectory(join(root,"blobs"));
    await syncDirectory(root);
    const mimeType = dimensions.kind === "audio" && signature.mimeType === "video/mp4" ? "audio/mp4" : dimensions.kind === "audio" && signature.mimeType === "video/webm" ? "audio/webm" : dimensions.kind === "video" && signature.mimeType === "audio/ogg" ? "video/ogg" : signature.mimeType;
    return { name, ...dimensions, mimeType, digest, bytes: buffer.length, relativePath };
  } finally { await unlink(staging).catch(() => {}); }
}
