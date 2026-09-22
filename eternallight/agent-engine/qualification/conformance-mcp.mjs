import { nativeBinding } from "../src/native.mjs";

await nativeBinding().runQualificationMcp(JSON.stringify({
  mode: "delegation", receipt: process.env.ENGINE_CONFORMANCE_RECEIPT,
  proofPath: process.env.ENGINE_CONFORMANCE_PROOF_PATH,
  followup: process.env.ENGINE_CONFORMANCE_FOLLOWUP,
  gatePath: process.env.ENGINE_CONFORMANCE_GATE_PATH,
  tracePath: process.env.ENGINE_CONFORMANCE_TRACE_PATH || undefined,
  releasePath: process.env.ENGINE_CONFORMANCE_RELEASE_PATH || undefined,
}));
