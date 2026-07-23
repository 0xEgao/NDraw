import type {
  CanvasAction,
  ChatEvent,
  DrawingRatingView,
  PhaseView,
  PlayerView,
  RoomSettings,
  RoomSnapshot,
  ServerMessage,
  TurnResultView,
  WordOptions,
} from "./protocol.ts";

export interface RoomState {
  roomCode: string;
  selfPlayerId: number | null;
  settings: RoomSettings | null;
  players: PlayerView[];
  phase: PhaseView | null;
  canvasActions: CanvasAction[];
  wordOptions: WordOptions | null;
  secretWord: string | null;
  chatHistory: ChatEvent[];
  turnResult: TurnResultView | null;
  drawingRating: DrawingRatingView | null;
  lastError: string | null;
  byeReason: number | null;
  ready: boolean;
}

export function initialRoomState(roomCode: string): RoomState {
  return {
    roomCode,
    selfPlayerId: null,
    settings: null,
    players: [],
    phase: null,
    canvasActions: [],
    wordOptions: null,
    secretWord: null,
    chatHistory: [],
    turnResult: null,
    drawingRating: null,
    lastError: null,
    byeReason: null,
    ready: false,
  };
}

function fromSnapshot(
  previous: RoomState,
  selfPlayerId: number,
  snapshot: RoomSnapshot,
  roomCode = previous.roomCode,
): RoomState {
  return {
    ...previous,
    roomCode,
    selfPlayerId,
    settings: snapshot.settings,
    players: snapshot.players,
    phase: snapshot.phase,
    canvasActions: snapshot.canvas.actions,
    wordOptions: snapshot.wordOptions,
    secretWord: snapshot.secretWord,
    chatHistory: snapshot.chatHistory,
    turnResult: snapshot.turnResult,
    drawingRating: snapshot.drawingRating,
    lastError: null,
    byeReason: null,
    ready: true,
  };
}

function upsertPlayer(players: PlayerView[], incoming: PlayerView): PlayerView[] {
  const existingIndex = players.findIndex((player) => player.playerId === incoming.playerId);
  if (existingIndex < 0) return [...players, incoming];
  return players.map((player) => player.playerId === incoming.playerId ? incoming : player);
}

function applyDraw(actions: CanvasAction[], message: Extract<ServerMessage, { kind: "draw" }>): CanvasAction[] {
  const operation = message.operation;
  switch (operation.kind) {
    case "begin":
      return [
        ...actions.filter((action) => action.kind !== "stroke" || action.stroke.strokeId !== operation.strokeId),
        {
          kind: "stroke",
          stroke: {
            strokeId: operation.strokeId,
            color: operation.color,
            width: operation.width,
            points: [operation.start],
          },
        },
      ];
    case "points":
      return actions.map((action) => action.kind === "stroke" && action.stroke.strokeId === operation.strokeId
        ? { ...action, stroke: { ...action.stroke, points: [...action.stroke.points, ...operation.points] } }
        : action);
    case "end":
      // Produce a final authoritative repaint after the drawer stops skipping
      // partial echo renders during their local pointer gesture.
      return [...actions];
    case "undo":
      return actions.slice(0, -1);
    case "clear":
      return [];
    case "fill":
      return [...actions, { kind: "fill", color: operation.color, at: operation.at }];
  }
}

/** Applies one authoritative server event to the browser's room projection. */
export function reduceRoom(state: RoomState, message: ServerMessage): RoomState {
  switch (message.kind) {
    case "welcome":
      return fromSnapshot(state, message.playerId, message.snapshot, message.roomCode);
    case "resume":
      return fromSnapshot(state, message.playerId, message.snapshot);
    case "playerJoined":
      return { ...state, players: upsertPlayer(state.players, message.player) };
    case "playerLeft":
      return {
        ...state,
        players: state.players.map((player) => player.playerId === message.playerId
          ? { ...player, connected: false }
          : player),
      };
    case "hostChanged":
      return {
        ...state,
        players: state.players.map((player) => ({
          ...player,
          isHost: player.playerId === message.playerId,
        })),
      };
    case "phaseChanged": {
      const startsNewCanvas = message.phase.phase === "choosingWord"
        && message.phase.turnId !== state.phase?.turnId;
      return {
        ...state,
        phase: message.phase,
        canvasActions: startsNewCanvas ? [] : state.canvasActions,
        wordOptions: message.phase.phase === "choosingWord" ? state.wordOptions : null,
        secretWord: message.phase.phase === "drawing" ? state.secretWord : null,
        players: state.players.map((player) => ({
          ...player,
          hasGuessed: message.phase.phase === "drawing" ? player.hasGuessed : false,
        })),
        turnResult: startsNewCanvas ? null : state.turnResult,
        drawingRating: startsNewCanvas ? null : state.drawingRating,
      };
    }
    case "wordOptions":
      return { ...state, wordOptions: message.options };
    case "secretWord":
      return { ...state, secretWord: message.word };
    case "hintRevealed":
      return state.phase?.turnId === message.turnId
        ? { ...state, phase: { ...state.phase, maskedWord: message.maskedWord } }
        : state;
    case "draw":
      return state.phase?.turnId === message.turnId
        ? {
            ...state,
            canvasActions: applyDraw(state.canvasActions, message),
          }
        : state;
    case "chat":
      return { ...state, chatHistory: [...state.chatHistory, message.event].slice(-64) };
    case "guessResult":
      return typeof message.outcome === "object"
        ? {
            ...state,
            players: state.players.map((player) => player.playerId === message.playerId
              ? { ...player, hasGuessed: true }
              : player),
          }
        : state;
    case "scoreChanged":
      return {
        ...state,
        players: state.players.map((player) => player.playerId === message.playerId
          ? { ...player, score: message.totalScore }
          : player),
      };
    case "error":
      return { ...state, lastError: message.message };
    case "bye":
      return { ...state, byeReason: message.reason };
    case "turnResult":
      return {
        ...state,
        turnResult: message.result,
        drawingRating: { turnId: message.result.turnId, likes: 0, dislikes: 0, viewerVote: null },
      };
    case "drawingVoteChanged":
      if (state.drawingRating?.turnId !== message.turnId) return state;
      return {
        ...state,
        drawingRating: {
          turnId: message.turnId,
          likes: message.likes,
          dislikes: message.dislikes,
          viewerVote: message.playerId === state.selfPlayerId ? message.vote : state.drawingRating.viewerVote,
        },
      };
    case "ping":
      return state;
  }
}

export function roomPlayer(state: RoomState, playerId: number | null): PlayerView | null {
  if (playerId === null) return null;
  return state.players.find((player) => player.playerId === playerId) ?? null;
}

/** Orders the live rail by score while keeping join order stable for ties. */
export function rankPlayers(players: readonly PlayerView[]): PlayerView[] {
  return players
    .map((player, joinedIndex) => ({ player, joinedIndex }))
    .sort((left, right) => right.player.score - left.player.score || left.joinedIndex - right.joinedIndex)
    .map(({ player }) => player);
}
