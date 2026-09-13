export function safeSendToWebContents(webContents, channel, payload) {
  if (!webContents || webContents.isDestroyed?.()) return false;
  try {
    webContents.send(channel, payload);
    return true;
  } catch {
    return false;
  }
}
