import { nativeBinding } from "../native.mjs";
export async function durableReplaceJson(path, document) {
  await nativeBinding().durableReplaceJson(path, JSON.stringify(document));
}
