import { workerChannel } from "../src/bridge/worker-stdio.mjs";
const send = workerChannel(message => {
  if (message?.method === "start") send({ id: message.id, ...{ result: { baseUrl: "http://127.0.0.1:65534", token: "fixture-token" } } });
});
