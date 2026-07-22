import assert from "node:assert/strict";
import test from "node:test";
import { normalizeRoomCode, roomCodeFromPath, roomPath } from "../lib/roomPath.ts";

test("room codes produce canonical reload-safe paths", () => {
  assert.equal(normalizeRoomCode(" djbukn "), "DJBUKN");
  assert.equal(roomPath("djbukn"), "/DJBUKN");
  assert.equal(roomCodeFromPath("/DJBUKN"), "DJBUKN");
  assert.equal(roomCodeFromPath("/djbukn/"), "DJBUKN");
});

test("non-room paths are rejected", () => {
  assert.equal(normalizeRoomCode("ABC10I"), null);
  assert.equal(roomCodeFromPath("/"), null);
  assert.equal(roomCodeFromPath("/DJBUKN/settings"), null);
  assert.equal(roomPath("not-a-room"), "/");
});
