/** Minimal interactive client used for local end-to-end testing. */
import { createInterface } from "node:readline";
import WebSocket from "ws";
import type { RawData } from "ws";
import { decodeServerMessage, encodeClientMessage } from "../lib/protocol.ts";

const websocketUrl = process.argv[2];
const displayName = process.argv[3] ?? "Test Guest";
if (!websocketUrl) throw new Error("usage: live-test-client.ts <ws-url> [display-name]");

const socket = new WebSocket(websocketUrl, { origin: "http://localhost:3100" });
const send = (message: Parameters<typeof encodeClientMessage>[0]) => socket.send(encodeClientMessage(message));

socket.on("open", () => {
  send({
    kind: "hello",
    clientToken: crypto.randomUUID(),
    profile: { displayName, avatar: [9, 8, 7, 6, 5, 4, 3, 2] },
  });
});

socket.on("message", (data: RawData) => {
  const message = decodeServerMessage(data instanceof Uint8Array ? data : new Uint8Array(data as ArrayBuffer));
  console.log(JSON.stringify(message));
  if (message.kind === "ping") send({ kind: "pong", nonce: message.nonce });
  if (message.kind === "wordOptions") send({ kind: "pickWord", choice: 0 });
});

socket.on("error", (error) => {
  console.error(error.message);
});

const input = createInterface({ input: process.stdin, terminal: false });
input.on("line", (line) => {
  const [command, ...words] = line.trim().split(/\s+/);
  const text = words.join(" ");
  if (command === "guess" && text) send({ kind: "guess", text });
  else if (command === "chat" && text) send({ kind: "chat", text });
  else if (command === "close") socket.close(1000, "test complete");
});
