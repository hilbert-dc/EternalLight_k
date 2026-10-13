import { chatCompletionsCodec } from "./chat-completions.mjs";
import { generateContentCodec } from "./generate-content.mjs";
import { messagesCodec } from "./messages.mjs";
import { responsesCodec } from "./responses.mjs";

const CODECS = new Map([
  [responsesCodec.id, responsesCodec],
  [chatCompletionsCodec.id, chatCompletionsCodec],
  [messagesCodec.id, messagesCodec],
  [generateContentCodec.id, generateContentCodec],
]);

export function getCodec(protocol) {
  const codec = CODECS.get(protocol);
  if (!codec) throw new Error(`no codec for protocol: ${protocol}`);
  return codec;
}

export { chatCompletionsCodec, generateContentCodec, messagesCodec, responsesCodec };
