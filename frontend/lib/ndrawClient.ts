import { AvatarBytes, ClientMessage, ServerMessage, decodeServerMessage, encodeClientMessage } from "./protocol.ts";

export type ConnectionState = "connecting" | "open" | "closed";

/** Thin WebSocket transport; game state stays in the React room store. */
export class NDrawClient {
  #socket: WebSocket | null = null;
  readonly #onMessage: (message: ServerMessage) => void;
  readonly #onState: (state: ConnectionState) => void;

  constructor(onMessage: (message: ServerMessage) => void, onState: (state: ConnectionState) => void) {
    this.#onMessage = onMessage;
    this.#onState = onState;
  }

  connect(url: string, clientToken: string, displayName: string, avatar: AvatarBytes): void {
    this.close();
    this.#onState("connecting");
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    this.#socket = socket;
    socket.addEventListener("open", () => {
      if (this.#socket !== socket) return;
      this.#onState("open");
      this.send({ kind: "hello", clientToken, profile: { displayName, avatar } });
    });
    socket.addEventListener("message", (event) => {
      if (this.#socket !== socket || !(event.data instanceof ArrayBuffer)) return;
      try {
        const message = decodeServerMessage(event.data);
        if (message.kind === "ping") this.send({ kind: "pong", nonce: message.nonce });
        this.#onMessage(message);
      } catch {
        socket.close(1002, "invalid binary protocol frame");
      }
    });
    socket.addEventListener("close", () => {
      if (this.#socket !== socket) return;
      this.#socket = null;
      this.#onState("closed");
    });
    socket.addEventListener("error", () => socket.close());
  }

  send(message: ClientMessage): boolean {
    if (!this.#socket || this.#socket.readyState !== WebSocket.OPEN) return false;
    this.#socket.send(encodeClientMessage(message));
    return true;
  }

  close(): void {
    const socket = this.#socket;
    this.#socket = null;
    if (socket) socket.close(1000, "client navigation");
  }
}

export function roomWebSocketUrl(roomCode: string): string {
  const apiBase = process.env.NEXT_PUBLIC_NDRAW_API_BASE ?? "http://127.0.0.1:3000";
  const url = new URL(apiBase);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.pathname = `/v1/ws/${roomCode}`;
  url.search = "";
  return url.toString();
}
