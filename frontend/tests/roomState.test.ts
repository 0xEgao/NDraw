import assert from "node:assert/strict";
import test from "node:test";
import type { PhaseView, RoomSnapshot } from "../lib/protocol.ts";
import { initialRoomState, rankPlayers, reduceRoom } from "../lib/roomState.ts";

const lobby: PhaseView = {
  phase: "lobby",
  round: 0,
  totalRounds: 3,
  drawer: null,
  turnId: null,
  deadlineUnixMs: 1_800_000_000_000,
  maskedWord: null,
};

const snapshot: RoomSnapshot = {
  settings: { rounds: 3, drawSeconds: 100, wordChoices: 4, maxPlayers: 12 },
  players: [{
    playerId: 7,
    profile: { displayName: "Ada", avatar: [1, 2, 3, 4, 5, 6, 7, 8] },
    score: 0,
    connected: true,
    isHost: true,
    hasGuessed: false,
  }],
  phase: lobby,
  canvas: { backgroundColor: 0xffffff, strokes: [] },
  wordOptions: null,
  secretWord: null,
  chatHistory: [],
  turnResult: null,
  drawingRating: null,
};

test("welcome replaces placeholders with the authoritative snapshot", () => {
  const state = reduceRoom(initialRoomState("ABC234"), {
    kind: "welcome",
    playerId: 7,
    roomCode: "ABC234",
    snapshot,
  });
  assert.equal(state.ready, true);
  assert.equal(state.players.length, 1);
  assert.equal(state.players[0]?.profile.displayName, "Ada");
  assert.equal(state.phase?.phase, "lobby");
});

test("turn results and viewer votes remain authoritative", () => {
  let state = reduceRoom(initialRoomState("ABC234"), {
    kind: "welcome",
    playerId: 7,
    roomCode: "ABC234",
    snapshot,
  });
  state = reduceRoom(state, {
    kind: "turnResult",
    result: { turnId: 3, word: "cat", awards: [{ playerId: 7, points: 200, reason: "drawerBonus" }] },
  });
  state = reduceRoom(state, { kind: "drawingVoteChanged", turnId: 3, playerId: 7, vote: "like", likes: 2, dislikes: 1 });
  assert.equal(state.turnResult?.awards[0]?.points, 200);
  assert.deepEqual(state.drawingRating, { turnId: 3, likes: 2, dislikes: 1, viewerVote: "like" });
});

test("draw events reconstruct the shared canvas", () => {
  const drawing: PhaseView = { ...lobby, phase: "drawing", round: 1, drawer: 7, turnId: 3, maskedWord: "___" };
  let state = reduceRoom(initialRoomState("ABC234"), { kind: "welcome", playerId: 7, roomCode: "ABC234", snapshot: { ...snapshot, phase: drawing } });
  state = reduceRoom(state, { kind: "draw", playerId: 7, turnId: 3, operation: { kind: "begin", strokeId: 9, color: 0x7656df, width: 8, start: { x: 10, y: 20 } } });
  state = reduceRoom(state, { kind: "draw", playerId: 7, turnId: 3, operation: { kind: "points", strokeId: 9, sequence: 0, points: [{ x: 11, y: 21 }] } });
  state = reduceRoom(state, { kind: "draw", playerId: 7, turnId: 3, operation: { kind: "end", strokeId: 9, sequence: 1 } });
  assert.deepEqual(state.strokes[0]?.points, [{ x: 10, y: 20 }, { x: 11, y: 21 }]);
});

test("fill and clear events update the authoritative canvas background", () => {
  const drawing: PhaseView = { ...lobby, phase: "drawing", round: 1, drawer: 7, turnId: 3, maskedWord: "___" };
  let state = reduceRoom(initialRoomState("ABC234"), { kind: "welcome", playerId: 7, roomCode: "ABC234", snapshot: { ...snapshot, phase: drawing } });
  state = reduceRoom(state, { kind: "draw", playerId: 7, turnId: 3, operation: { kind: "fill", color: 0x123456 } });
  assert.equal(state.backgroundColor, 0x123456);
  state = reduceRoom(state, { kind: "draw", playerId: 7, turnId: 3, operation: { kind: "clear" } });
  assert.equal(state.backgroundColor, 0xffffff);
});

test("a new choosing phase clears the previous turn canvas", () => {
  const state = {
    ...initialRoomState("ABC234"),
    phase: { ...lobby, phase: "roundEnd" as const, turnId: 2 },
    strokes: [{ strokeId: 1, color: 0, width: 4, points: [{ x: 1, y: 1 }] }],
  };
  const next = reduceRoom(state, { kind: "phaseChanged", phase: { ...lobby, phase: "choosingWord", round: 2, drawer: 7, turnId: 3 } });
  assert.deepEqual(next.strokes, []);
});

test("player ranking follows score and remains stable for ties", () => {
  const ada = snapshot.players[0];
  assert.ok(ada);
  const players = [
    { ...ada, playerId: 1, profile: { ...ada.profile, displayName: "First" }, score: 80 },
    { ...ada, playerId: 2, profile: { ...ada.profile, displayName: "Leader" }, score: 240 },
    { ...ada, playerId: 3, profile: { ...ada.profile, displayName: "Tie" }, score: 80 },
  ];
  assert.deepEqual(rankPlayers(players).map((player) => player.playerId), [2, 1, 3]);
});
