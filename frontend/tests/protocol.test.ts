import assert from "node:assert/strict";
import test from "node:test";
import { decodeServerMessage, encodeClientMessage } from "../lib/protocol.ts";

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

test("matches the Rust StartGame golden fixture", () => {
  assert.equal(hex(encodeClientMessage({ kind: "startGame" })), "0202");
});

test("matches the Rust KickPlayer golden fixture", () => {
  assert.equal(hex(encodeClientMessage({ kind: "kickPlayer", playerId: 42 })), "02072a");
});

test("matches the Rust Hello golden fixture", () => {
  const frame = encodeClientMessage({
    kind: "hello",
    clientToken: "00112233-4455-6677-8899-aabbccddeeff",
    profile: { displayName: "Ada", avatar: [1, 2, 3, 4, 5, 6, 7, 8] },
  });
  assert.equal(hex(frame), "02011000112233445566778899aabbccddeeff034164610102030405060708");
});

test("decodes the Rust Ping golden fixture", () => {
  assert.deepEqual(decodeServerMessage(Uint8Array.from([0x02, 0x8f, 0x2a])), { kind: "ping", nonce: 42 });
});

test("encodes drawing votes with the frozen opcode", () => {
  assert.equal(hex(encodeClientMessage({ kind: "voteDrawing", turnId: 4, vote: "like" })), "020a040100");
  assert.equal(hex(encodeClientMessage({ kind: "voteDrawing", turnId: 4, vote: null })), "020a0400");
});

test("matches the Rust authoritative canvas-fill fixtures", () => {
  assert.equal(hex(encodeClientMessage({ kind: "draw", operation: { kind: "fill", color: 42, at: { x: 10, y: 20 } } })), "0204052a0a14");
  assert.deepEqual(
    decodeServerMessage(Uint8Array.from([0x02, 0x8a, 0x07, 0x04, 0x05, 0x2a, 0x0a, 0x14])),
    { kind: "draw", playerId: 7, turnId: 4, operation: { kind: "fill", color: 42, at: { x: 10, y: 20 } } },
  );
});

test("decodes room-visible guesses as guess chat lines", () => {
  assert.deepEqual(
    decodeServerMessage(Uint8Array.from([0x02, 0x8b, 0x07, 0x01, 0x02, 0x6e, 0x6f])),
    { kind: "chat", event: { playerId: 7, kind: "guess", text: "no" } },
  );
});

test("decodes authoritative drawing vote updates", () => {
  assert.deepEqual(
    decodeServerMessage(Uint8Array.from([0x02, 0x92, 0x04, 0x07, 0x01, 0x00, 0x01, 0x00])),
    { kind: "drawingVoteChanged", turnId: 4, playerId: 7, vote: "like", likes: 1, dislikes: 0 },
  );
});

test("rejects trailing and unknown bytes", () => {
  assert.throws(() => decodeServerMessage(Uint8Array.from([0x02, 0x8f, 0x2a, 0x00])), /trailing/);
  assert.throws(() => decodeServerMessage(Uint8Array.from([0x02, 0xff])), /unknown server opcode/);
});
