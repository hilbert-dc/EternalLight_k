import { workerChannel } from "../src/bridge/worker-stdio.mjs";
const send = workerChannel(message => {
  if (message?.method === "start") send({ id: message.id, ...{ error: { message: "fixture start failed", code: "fixture" } } });
});
