import { randomUUID } from "node:crypto";
import { rename } from "node:fs/promises";
import { listCodexConnections, loadCodexEvidence } from "./codex-runtime.mjs";

const CACHE_ERROR = "本地模型能力认证缓存无法验证。作品仍可打开和手动编辑，请在设置中重新验证模型后应用配置。";

export async function readModelEvidenceCache(path) {
  try { return { records: await loadCodexEvidence(path), error: null }; }
  catch { return { records: [], error: CACHE_ERROR }; }
}

export async function listModelConnections({ evidencePath, ...options }) {
  const snapshot = await readModelEvidenceCache(evidencePath);
  const connections = listCodexConnections({ ...options, evidence: snapshot.records });
  return snapshot.error ? connections.map(connection => ({ ...connection,
    status: "unverified", verifiedCapabilities: [], verifiedProfiles: [], failedCapabilities: [], selectedProtocol: null, certificationIds: [],
  })) : connections;
}

// Only explicit re-verification can retire rejected cache bytes. No field migration.
export async function prepareEvidenceReverification(path) {
  const snapshot = await readModelEvidenceCache(path);
  if (!snapshot.error) return null;
  const rejectedPath = `${path}.rejected-${randomUUID()}`;
  await rename(path, rejectedPath);
  return rejectedPath;
}
