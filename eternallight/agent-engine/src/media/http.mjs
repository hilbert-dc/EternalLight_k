import { isIP } from "node:net";

export function mediaError(code, message, properties = {}) { return Object.assign(new Error(message), { code, ...properties }); }
export function mediaEndpoint(value, allowLocalhost = false) {
  let url;try{url=new URL(value);}catch{throw mediaError("MEDIA_ENDPOINT_INVALID","媒体服务地址无效");}
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  if (url.username || url.password || url.search || url.hash || (url.protocol !== "https:" && !(allowLocalhost && local && url.protocol === "http:"))) throw mediaError("MEDIA_ENDPOINT_INVALID", "媒体服务地址必须是 HTTPS");
  return url.href.replace(/\/$/, "");
}
export async function boundedBody(response, maximumBytes) {
  const length = Number(response.headers.get("content-length"));
  if (Number.isFinite(length) && length > maximumBytes) throw mediaError("MEDIA_OUTPUT_TOO_LARGE", "媒体响应超过大小限制");
  const chunks = []; let size = 0;
  if (!response.body) throw mediaError("MEDIA_RESPONSE_INVALID", "媒体服务没有返回内容");
  const reader = response.body.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      size += value.byteLength;
      if (size > maximumBytes) { await reader.cancel(); throw mediaError("MEDIA_OUTPUT_TOO_LARGE", "媒体响应超过大小限制"); }
      chunks.push(Buffer.from(value));
    }
  } finally { reader.releaseLock(); }
  return Buffer.concat(chunks, size);
}
export function mediaTransport({ baseUrl, apiKey, fetchImpl = fetch, headers = {}, allowLocalhost = false, timeoutMs = 120_000 }) {
  const endpoint = mediaEndpoint(baseUrl, allowLocalhost);
  return async (path, { method = "GET", body, signal, binary = false, maximumBytes = 64 * 1024 * 1024 } = {}) => {
    if (typeof apiKey !== "string" || !apiKey.trim()) throw mediaError("MEDIA_CREDENTIAL_MISSING", "媒体服务尚未配置密钥", { notSent: true });
    const controller = new AbortController();
    const abort = () => controller.abort(); signal?.addEventListener("abort", abort, { once: true });
    if (signal?.aborted) controller.abort();
    const timer = setTimeout(abort, timeoutMs); timer.unref?.();
    try {
      const multipart = body instanceof FormData;
      const response = await fetchImpl(`${endpoint}${path}`, { method, headers: { Authorization: `Bearer ${apiKey}`, ...headers, ...(body && !multipart ? { "Content-Type": "application/json" } : {}) }, body: body ? multipart ? body : JSON.stringify(body) : undefined, signal: controller.signal, redirect: "error" });
      const headerId=response.headers.get("x-request-id"),requestId=headerId&&/^[A-Za-z0-9._:-]{1,180}$/.test(headerId)?headerId:null;
      if (!response.ok) {
        // Provider bodies can contain prompts or secrets: never copy them into logs/errors.
        await response.body?.cancel();
        throw mediaError(`MEDIA_HTTP_${response.status}`, `媒体服务请求失败（${response.status}）`, { rejected: [400, 401, 403, 404, 405, 413, 422, 429].includes(response.status) });
      }
      if (response.status === 204) return { requestId, data: null };
      const bytes = await boundedBody(response, maximumBytes);
      return { requestId, data: binary ? bytes : JSON.parse(bytes.toString("utf8")), mimeType: response.headers.get("content-type")?.split(";")[0] ?? null };
    } catch (error) {
      if (error?.code?.startsWith("MEDIA_")) throw error;
      throw mediaError("MEDIA_TRANSPORT_UNCERTAIN", "媒体服务响应未能确认");
    } finally { clearTimeout(timer); signal?.removeEventListener("abort", abort); }
  };
}
export async function downloadMedia(urlString, { fetchImpl = fetch, signal, allowedHosts, maximumBytes = 512 * 1024 * 1024, timeoutMs = 120_000 }) {
  let url;try{url=new URL(urlString);}catch{throw mediaError("MEDIA_OUTPUT_LOCATION_INVALID","生成文件地址无效");}
  if (url.protocol !== "https:" || url.username || url.password || isIP(url.hostname) || !allowedHosts?.some(host => url.hostname === host || url.hostname.endsWith(`.${host}`))) throw mediaError("MEDIA_OUTPUT_LOCATION_INVALID", "生成文件地址不属于已登记的媒体存储");
  const controller=new AbortController(), abort=()=>controller.abort();signal?.addEventListener("abort",abort,{once:true});if(signal?.aborted)abort();const timer=setTimeout(abort,timeoutMs);timer.unref?.();
  try {
    const response = await fetchImpl(url, { signal:controller.signal, redirect: "error" });
    if (!response.ok) throw mediaError("MEDIA_DOWNLOAD_FAILED", "生成文件下载失败，可从原作业重新获取");
    return { bytes: await boundedBody(response, maximumBytes), mimeType: response.headers.get("content-type")?.split(";")[0] ?? "application/octet-stream" };
  } finally {clearTimeout(timer);signal?.removeEventListener("abort",abort);}
}
export function requiredText(value, label, maximum = 32000) {
  if (typeof value !== "string" || !value.trim() || value.length > maximum) throw mediaError("MEDIA_REQUEST_INVALID", `${label}无效`, { notSent: true });
  return value;
}
export function oneOf(value, values, label) { if (!values.includes(value)) throw mediaError("MEDIA_REQUEST_INVALID", `${label}不受支持`, { notSent: true }); return value; }
