import { createHash, randomUUID } from "node:crypto";
import { readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function certificationPath(directory, policy) {
  const name = policy.modelCertificationsFile;
  if (typeof name !== "string" || !/^model-certifications(?:-[a-f0-9]{64})?\.json$/.test(name)) throw new Error("invalid certification snapshot filename");
  return join(directory, name);
}

// Publish immutable content first. The policy rename is the sole commit point;
// a crash beforehand leaves the previous snapshot fully usable.
export function publishCertificationSnapshot(policyPath, policy, document) {
  const content = `${JSON.stringify(document, null, 2)}\n`;
  const hash = createHash("sha256").update(content).digest("hex");
  const nextPolicy = { ...policy, modelCertificationsFile: `model-certifications-${hash}.json`, modelCertificationsSha256: hash };
  const path = certificationPath(dirname(policyPath), nextPolicy);
  try {
    writeFileSync(path, content, { flag: "wx", mode: 0o600, flush: true });
  } catch (error) {
    if (error.code !== "EEXIST" || readFileSync(path, "utf8") !== content) throw error;
  }
  const temporary = `${policyPath}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, `${JSON.stringify(nextPolicy, null, 2)}\n`, { flag: "wx", mode: 0o600, flush: true });
    renameSync(temporary, policyPath);
  } finally {
    rmSync(temporary, { force: true });
  }
  return nextPolicy;
}
