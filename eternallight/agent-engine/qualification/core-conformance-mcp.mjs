import { nativeBinding } from "../src/native.mjs";

await nativeBinding().runQualificationMcp(JSON.stringify({
  mode: "core", receipt: process.env.ENGINE_CORE_RECEIPT,
  proofPath: process.env.ENGINE_CORE_PROOF_PATH,
  nonce: process.env.ENGINE_CORE_NONCE, instruction: process.env.ENGINE_CORE_INSTRUCTION,
}));
