import { pathToFileURL } from "node:url";

export function isAllowedRendererNavigation(targetUrl, { isPackaged, rendererUrl, packagedEntry }) {
  let target;
  try {
    target = new URL(targetUrl);
  } catch {
    return false;
  }
  if (isPackaged) return target.href === pathToFileURL(packagedEntry).href;
  const renderer = new URL(rendererUrl);
  return target.protocol === renderer.protocol && target.origin === renderer.origin;
}

export function isTrustedIpcSender(event, expectedWebContents, options) {
  const sender = event?.sender;
  const senderUrl = event?.senderFrame?.url ?? sender?.getURL?.() ?? "";
  return Boolean(expectedWebContents && sender === expectedWebContents && !sender?.isDestroyed?.() && isAllowedRendererNavigation(senderUrl, options));
}
