import { Reader, Writer } from "./postcard.ts";

export const PROTOCOL_VERSION = 2;
export const MAX_FRAME_BYTES = 16 * 1024;
export const MAX_POINTS_PER_BATCH = 64;

export type AvatarBytes = readonly [number, number, number, number, number, number, number, number];
export type Point = { x: number; y: number };
export type DrawOp =
  | { kind: "begin"; strokeId: number; color: number; width: number; start: Point }
  | { kind: "points"; strokeId: number; sequence: number; points: Point[] }
  | { kind: "end"; strokeId: number; sequence: number }
  | { kind: "undo" }
  | { kind: "clear" }
  | { kind: "fill"; color: number; at: Point };

export interface PlayerProfile { displayName: string; avatar: AvatarBytes }
export interface RoomSettings { rounds: number; drawSeconds: number; wordChoices: number; maxPlayers: number }
export interface PlayerView { playerId: number; profile: PlayerProfile; score: number; connected: boolean; isHost: boolean; hasGuessed: boolean }
export type GamePhase = "lobby" | "choosingWord" | "drawing" | "roundEnd" | "gameOver";
export interface PhaseView { phase: GamePhase; round: number; totalRounds: number; drawer: number | null; turnId: number | null; deadlineUnixMs: number | null; maskedWord: string | null }
export interface Stroke { strokeId: number; color: number; width: number; points: Point[] }
export type CanvasAction =
  | { kind: "stroke"; stroke: Stroke }
  | { kind: "fill"; color: number; at: Point };
export interface ChatEvent { playerId: number; kind: "chat" | "guess"; text: string }
export interface WordOptions { turnId: number; words: string[] }
export type AwardReason = "correctGuess" | "drawerBonus";
export interface TurnAward { playerId: number; points: number; reason: AwardReason }
export interface TurnResultView { turnId: number; word: string; awards: TurnAward[] }
export type DrawingVote = "like" | "dislike";
export interface DrawingRatingView { turnId: number; likes: number; dislikes: number; viewerVote: DrawingVote | null }
export interface RoomSnapshot {
  settings: RoomSettings;
  players: PlayerView[];
  phase: PhaseView;
  canvas: { actions: CanvasAction[] };
  wordOptions: WordOptions | null;
  secretWord: string | null;
  chatHistory: ChatEvent[];
  turnResult: TurnResultView | null;
  drawingRating: DrawingRatingView | null;
}

export type ClientMessage =
  | { kind: "hello"; clientToken: string; profile: PlayerProfile }
  | { kind: "startGame" }
  | { kind: "pickWord"; choice: number }
  | { kind: "draw"; operation: DrawOp }
  | { kind: "guess"; text: string }
  | { kind: "chat"; text: string }
  | { kind: "kickPlayer"; playerId: number }
  | { kind: "rematch" }
  | { kind: "pong"; nonce: number }
  | { kind: "voteDrawing"; turnId: number; vote: DrawingVote | null };

export type ServerMessage =
  | { kind: "welcome"; playerId: number; roomCode: string; snapshot: RoomSnapshot }
  | { kind: "resume"; playerId: number; snapshot: RoomSnapshot }
  | { kind: "playerJoined"; player: PlayerView }
  | { kind: "playerLeft"; playerId: number }
  | { kind: "hostChanged"; playerId: number }
  | { kind: "phaseChanged"; phase: PhaseView }
  | { kind: "wordOptions"; options: WordOptions }
  | { kind: "secretWord"; turnId: number; word: string }
  | { kind: "hintRevealed"; turnId: number; maskedWord: string }
  | { kind: "draw"; playerId: number; turnId: number; operation: DrawOp }
  | { kind: "chat"; event: ChatEvent }
  | { kind: "guessResult"; playerId: number; outcome: "incorrect" | "close" | { correct: number } }
  | { kind: "scoreChanged"; playerId: number; totalScore: number; delta: number }
  | { kind: "error"; code: number; message: string }
  | { kind: "ping"; nonce: number }
  | { kind: "bye"; reason: number }
  | { kind: "turnResult"; result: TurnResultView }
  | { kind: "drawingVoteChanged"; turnId: number; playerId: number; vote: DrawingVote | null; likes: number; dislikes: number };

const CLIENT_OPCODE = { hello: 0x01, startGame: 0x02, pickWord: 0x03, draw: 0x04, guess: 0x05, chat: 0x06, kickPlayer: 0x07, rematch: 0x08, pong: 0x09, voteDrawing: 0x0a } as const;

function uuidBytes(uuid: string): Uint8Array {
  const compact = uuid.replaceAll("-", "");
  if (!/^[0-9a-fA-F]{32}$/.test(compact)) throw new Error("client token must be a UUID");
  return Uint8Array.from(Array.from({ length: 16 }, (_, index) => Number.parseInt(compact.slice(index * 2, index * 2 + 2), 16)));
}

function writeProfile(writer: Writer, profile: PlayerProfile): void {
  writer.string(profile.displayName).fixed(Uint8Array.from(profile.avatar));
}

function writePoint(writer: Writer, point: Point): void {
  writer.varint(point.x).varint(point.y);
}

function writeDraw(writer: Writer, operation: DrawOp): void {
  switch (operation.kind) {
    case "begin":
      writer.varint(0).varint(operation.strokeId).varint(operation.color).u8(operation.width);
      writePoint(writer, operation.start);
      return;
    case "points":
      writer.varint(1).varint(operation.strokeId).varint(operation.sequence).vector(operation.points, writePoint);
      return;
    case "end": writer.varint(2).varint(operation.strokeId).varint(operation.sequence); return;
    case "undo": writer.varint(3); return;
    case "clear": writer.varint(4); return;
    case "fill":
      writer.varint(5).varint(operation.color);
      writePoint(writer, operation.at);
      return;
  }
}

export function encodeClientMessage(message: ClientMessage): Uint8Array<ArrayBuffer> {
  const writer = new Writer().u8(PROTOCOL_VERSION).u8(CLIENT_OPCODE[message.kind]);
  switch (message.kind) {
    case "hello": {
      const token = uuidBytes(message.clientToken);
      // `uuid::Uuid` uses Serde's byte-string representation, so Postcard
      // writes its 16-byte length before the UUID body.
      writer.varint(token.length).fixed(token);
      writeProfile(writer, message.profile);
      break;
    }
    case "pickWord": writer.u8(message.choice); break;
    case "draw": writeDraw(writer, message.operation); break;
    case "guess": case "chat": writer.string(message.text); break;
    case "kickPlayer": writer.varint(message.playerId); break;
    case "pong": writer.varint(message.nonce); break;
    case "voteDrawing":
      writer.varint(message.turnId).option(message.vote, (value, vote) => value.varint(vote === "like" ? 0 : 1));
      break;
    case "startGame": case "rematch": break;
  }
  const frame = writer.finish();
  if (frame.byteLength > MAX_FRAME_BYTES) throw new Error(`frame exceeds ${MAX_FRAME_BYTES} bytes`);
  return frame;
}

function readPoint(reader: Reader): Point { return { x: reader.varint(), y: reader.varint() }; }
function readProfile(reader: Reader): PlayerProfile { return { displayName: reader.string(), avatar: Array.from(reader.fixed(8)) as unknown as AvatarBytes }; }
function readSettings(reader: Reader): RoomSettings { return { rounds: reader.u8(), drawSeconds: reader.varint(), wordChoices: reader.u8(), maxPlayers: reader.u8() }; }
function readPlayer(reader: Reader): PlayerView { return { playerId: reader.varint(), profile: readProfile(reader), score: reader.varint(), connected: reader.bool(), isHost: reader.bool(), hasGuessed: reader.bool() }; }
function readPhase(reader: Reader): PhaseView {
  const phases: GamePhase[] = ["lobby", "choosingWord", "drawing", "roundEnd", "gameOver"];
  const phase = phases[reader.varint()];
  if (!phase) throw new Error("server sent an unknown game phase");
  return { phase, round: reader.u8(), totalRounds: reader.u8(), drawer: reader.option((value) => value.varint()), turnId: reader.option((value) => value.varint()), deadlineUnixMs: reader.option((value) => value.varint()), maskedWord: reader.option((value) => value.string()) };
}
function readDraw(reader: Reader): DrawOp {
  const variant = reader.varint();
  if (variant === 0) return { kind: "begin", strokeId: reader.varint(), color: reader.varint(), width: reader.u8(), start: readPoint(reader) };
  if (variant === 1) return { kind: "points", strokeId: reader.varint(), sequence: reader.varint(), points: reader.vector(readPoint) };
  if (variant === 2) return { kind: "end", strokeId: reader.varint(), sequence: reader.varint() };
  if (variant === 3) return { kind: "undo" };
  if (variant === 4) return { kind: "clear" };
  if (variant === 5) return { kind: "fill", color: reader.varint(), at: readPoint(reader) };
  throw new Error(`server sent unknown DrawOp variant ${variant}`);
}
function readStroke(reader: Reader): Stroke { return { strokeId: reader.varint(), color: reader.varint(), width: reader.u8(), points: reader.vector(readPoint) }; }
function readCanvasAction(reader: Reader): CanvasAction {
  const variant = reader.varint();
  if (variant === 0) return { kind: "stroke", stroke: readStroke(reader) };
  if (variant === 1) return { kind: "fill", color: reader.varint(), at: readPoint(reader) };
  throw new Error(`server sent unknown CanvasAction variant ${variant}`);
}
function readChat(reader: Reader): ChatEvent {
  const playerId = reader.varint();
  const kind = reader.varint();
  if (kind !== 0 && kind !== 1) throw new Error(`server sent unknown chat kind ${kind}`);
  return { playerId, kind: kind === 0 ? "chat" : "guess", text: reader.string() };
}
function readWordOptions(reader: Reader): WordOptions { return { turnId: reader.varint(), words: reader.vector((value) => value.string()) }; }
function readDrawingVote(reader: Reader): DrawingVote {
  const value = reader.varint();
  if (value === 0) return "like";
  if (value === 1) return "dislike";
  throw new Error(`server sent unknown drawing vote ${value}`);
}
function readTurnResult(reader: Reader): TurnResultView {
  return {
    turnId: reader.varint(),
    word: reader.string(),
    awards: reader.vector((value) => {
      const playerId = value.varint();
      const points = value.varint();
      const reasonValue = value.varint();
      if (reasonValue !== 0 && reasonValue !== 1) throw new Error(`server sent unknown award reason ${reasonValue}`);
      return { playerId, points, reason: reasonValue === 0 ? "correctGuess" : "drawerBonus" };
    }),
  };
}
function readDrawingRating(reader: Reader): DrawingRatingView {
  return {
    turnId: reader.varint(),
    likes: reader.u8(),
    dislikes: reader.u8(),
    viewerVote: reader.option(readDrawingVote),
  };
}
function readSnapshot(reader: Reader): RoomSnapshot {
  return {
    settings: readSettings(reader),
    players: reader.vector(readPlayer),
    phase: readPhase(reader),
    canvas: { actions: reader.vector(readCanvasAction) },
    wordOptions: reader.option(readWordOptions),
    secretWord: reader.option((value) => value.string()),
    chatHistory: reader.vector(readChat),
    turnResult: reader.option(readTurnResult),
    drawingRating: reader.option(readDrawingRating),
  };
}

export function decodeServerMessage(frame: ArrayBuffer | Uint8Array): ServerMessage {
  const bytes = frame instanceof Uint8Array ? frame : new Uint8Array(frame);
  if (bytes.byteLength < 2 || bytes.byteLength > MAX_FRAME_BYTES) throw new Error("server frame has an invalid length");
  const reader = new Reader(bytes);
  const version = reader.u8();
  if (version !== PROTOCOL_VERSION) throw new Error(`unsupported protocol version ${version}`);
  const opcode = reader.u8();
  let message: ServerMessage;
  switch (opcode) {
    case 0x81: message = { kind: "welcome", playerId: reader.varint(), roomCode: reader.string(), snapshot: readSnapshot(reader) }; break;
    case 0x82: message = { kind: "resume", playerId: reader.varint(), snapshot: readSnapshot(reader) }; break;
    case 0x83: message = { kind: "playerJoined", player: readPlayer(reader) }; break;
    case 0x84: message = { kind: "playerLeft", playerId: reader.varint() }; break;
    case 0x85: message = { kind: "hostChanged", playerId: reader.varint() }; break;
    case 0x86: message = { kind: "phaseChanged", phase: readPhase(reader) }; break;
    case 0x87: message = { kind: "wordOptions", options: readWordOptions(reader) }; break;
    case 0x88: message = { kind: "secretWord", turnId: reader.varint(), word: reader.string() }; break;
    case 0x89: message = { kind: "hintRevealed", turnId: reader.varint(), maskedWord: reader.string() }; break;
    case 0x8a: message = { kind: "draw", playerId: reader.varint(), turnId: reader.varint(), operation: readDraw(reader) }; break;
    case 0x8b: message = { kind: "chat", event: readChat(reader) }; break;
    case 0x8c: {
      const playerId = reader.varint();
      const outcome = reader.varint();
      message = { kind: "guessResult", playerId, outcome: outcome === 0 ? "incorrect" : outcome === 1 ? "close" : outcome === 2 ? { correct: reader.varint() } : (() => { throw new Error(`unknown guess outcome ${outcome}`); })() };
      break;
    }
    case 0x8d: message = { kind: "scoreChanged", playerId: reader.varint(), totalScore: reader.varint(), delta: reader.signedVarint() }; break;
    case 0x8e: message = { kind: "error", code: reader.varint(), message: reader.string() }; break;
    case 0x8f: message = { kind: "ping", nonce: reader.varint() }; break;
    case 0x90: message = { kind: "bye", reason: reader.varint() }; break;
    case 0x91: message = { kind: "turnResult", result: readTurnResult(reader) }; break;
    case 0x92: message = {
      kind: "drawingVoteChanged",
      turnId: reader.varint(),
      playerId: reader.varint(),
      vote: reader.option(readDrawingVote),
      likes: reader.u8(),
      dislikes: reader.u8(),
    }; break;
    default: throw new Error(`unknown server opcode 0x${opcode.toString(16)}`);
  }
  reader.finish();
  return message;
}
