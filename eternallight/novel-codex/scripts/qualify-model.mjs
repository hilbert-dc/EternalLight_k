export async function qualifyModel(options = {}) {
  return certifyModel({ ...options, publish: false });
}

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { certifyModel } from "./certify-model.mjs";
import { parseModelCliOptions } from "./model-cli-options.mjs";
import { loadProjectDevelopmentEnvironment, resolveDevelopmentEnvironment } from "../electron/development-credentials.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = resolve(fileURLToPath(new URL("..", import.meta.url)));

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const { connectionId, profileId, help } = parseModelCliOptions(process.argv.slice(2));
  if (help) {
    process.stdout.write("Usage: npm run qualify:model -- [connection-id] [--profile profile-id]\nRuns conformance and writes a diagnostic report without publishing certification.\n");
    process.exit(0);
  }
  const environment = resolveDevelopmentEnvironment(loadProjectDevelopmentEnvironment(root, process.env));
  const result = await qualifyModel({ connectionId, environment, profileId });
  process.stdout.write(`${JSON.stringify(result)}\n`);
}
