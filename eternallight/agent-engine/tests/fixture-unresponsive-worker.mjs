process.on("message", (message) => {
  if (message?.method === "start" && process.connected) {
    process.send({ id: message.id, result: { baseUrl: "http://127.0.0.1:65534", token: "fixture-token" } });
  }
});
