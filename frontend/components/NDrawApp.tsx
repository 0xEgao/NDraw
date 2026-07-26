"use client";

import {
  ArrowRight,
  ChatCircleDots,
  Cat,
  Check,
  Copy,
  CrownSimple,
  GameController,
  Lightning,
  LinkSimple,
  LockKey,
  PaintBucket,
  PaperPlaneTilt,
  Sparkle,
  ShareNetwork,
  ThumbsDown,
  ThumbsUp,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { FormEvent, useEffect, useReducer, useRef, useState } from "react";
import { ConnectionState, NDrawClient, roomWebSocketUrl } from "../lib/ndrawClient";
import { DrawingVote, DrawOp, PhaseView, PlayerView, ServerMessage } from "../lib/protocol.ts";
import { normalizeRoomCode, roomCodeFromPath, roomPath } from "../lib/roomPath.ts";
import { initialRoomState, rankPlayers, reduceRoom, roomPlayer } from "../lib/roomState.ts";
import { Avatar, AvatarBytes } from "./Avatar";
import { AvatarPicker } from "./AvatarPicker";
import { DrawingStudio } from "./DrawingStudio";
import styles from "./ndraw.module.css";

const DEFAULT_AVATAR: AvatarBytes = [42, 178, 91, 217, 63, 144, 8, 201];
const ROUND_OPTIONS = [3, 5, 7] as const;
const HERO_AVATARS: { name: string; avatar: AvatarBytes }[] = [
  { name: "Ari", avatar: DEFAULT_AVATAR },
  { name: "Mira", avatar: [93, 12, 201, 48, 165, 77, 3, 220] },
  { name: "Kian", avatar: [31, 222, 84, 146, 7, 91, 12, 44] },
  { name: "Zoya", avatar: [165, 51, 210, 14, 89, 232, 5, 117] },
];

type View = "landing" | "room";
type MobilePanel = "players" | "chat" | null;
type ChatLine = {
  id: number;
  kind: "chat" | "guess" | "correct" | "system";
  playerId: number | null;
  text: string;
};

interface CreateRoomResponse {
  room_code: string;
  websocket_url: string;
  lobby_expires_at_ms: number;
}

function getClientToken(): string {
  const storageKey = "ndraw.client-token";
  const existing = window.localStorage.getItem(storageKey);
  if (existing) return existing;
  const created = crypto.randomUUID();
  window.localStorage.setItem(storageKey, created);
  return created;
}

function apiBase(): string {
  return process.env.NEXT_PUBLIC_NDRAW_API_BASE ?? "http://127.0.0.1:3000";
}

async function copyText(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // Clipboard access can be denied outside a secure context. Fall through to
    // the selection-based browser fallback used by older and embedded clients.
  }

  const input = document.createElement("textarea");
  input.value = value;
  input.readOnly = true;
  input.setAttribute("aria-hidden", "true");
  input.style.position = "fixed";
  input.style.left = "-9999px";
  document.body.appendChild(input);
  input.select();
  input.setSelectionRange(0, input.value.length);
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } catch {
    copied = false;
  }
  input.remove();
  return copied;
}

export function NDrawApp({ initialRoomCode = null }: { initialRoomCode?: string | null }) {
  const [view, setView] = useState<View>("landing");
  const [mode, setMode] = useState<"create" | "join">("create");
  // The first server and browser render must be identical. Restore browser
  // state only after hydration has completed.
  const [name, setName] = useState("");
  const [roomCode, setRoomCode] = useState("");
  const [activeRoom, setActiveRoom] = useState("");
  const [activeSocket, setActiveSocket] = useState("");
  const [avatar, setAvatar] = useState<AvatarBytes>(DEFAULT_AVATAR);
  const [rounds, setRounds] = useState<(typeof ROUND_OPTIONS)[number]>(3);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const [mobilePanel, setMobilePanel] = useState<MobilePanel>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const savedName = window.localStorage.getItem("ndraw.display-name") ?? "";
      const routedRoom = normalizeRoomCode(initialRoomCode ?? "");
      setName(savedName);
      if (!routedRoom) return;

      setMode("join");
      setRoomCode(routedRoom);
      if (savedName.trim()) {
        setActiveRoom(routedRoom);
        setActiveSocket(roomWebSocketUrl(routedRoom));
        setView("room");
      } else {
        setError("Enter your name to join this room.");
      }
    }, 0);
    return () => window.clearTimeout(timer);
  }, [initialRoomCode]);

  useEffect(() => {
    const handlePopState = () => {
      const routedRoom = roomCodeFromPath(window.location.pathname);
      if (!routedRoom) {
        setMobilePanel(null);
        setView("landing");
        setActiveRoom("");
        setActiveSocket("");
        return;
      }

      setMode("join");
      setRoomCode(routedRoom);
      const savedName = window.localStorage.getItem("ndraw.display-name") ?? "";
      if (!savedName.trim()) {
        setView("landing");
        setError("Enter your name to join this room.");
        return;
      }
      setName(savedName);
      setActiveRoom(routedRoom);
      setActiveSocket(roomWebSocketUrl(routedRoom));
      setView("room");
    };
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  const enterRoom = (code: string, websocketUrl: string) => {
    window.localStorage.setItem("ndraw.display-name", name.trim());
    const nextPath = roomPath(code);
    if (window.location.pathname !== nextPath) window.history.pushState({}, "", nextPath);
    else window.history.replaceState({}, "", nextPath);
    setActiveRoom(code);
    setActiveSocket(websocketUrl);
    setView("room");
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError("");
    if (!name.trim()) {
      setError("Give your artist a name first.");
      return;
    }
    if (mode === "join") {
      const normalized = normalizeRoomCode(roomCode);
      if (!normalized) {
        setError("Room codes are six letters or numbers.");
        return;
      }
      enterRoom(normalized, roomWebSocketUrl(normalized));
      return;
    }

    setCreating(true);
    try {
      const response = await fetch(`${apiBase()}/v1/rooms`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          client_token: getClientToken(),
          settings: { rounds, draw_seconds: 100, word_choices: 4, max_players: 12 },
        }),
      });
      if (!response.ok) throw new Error(`room creation returned ${response.status}`);
      const created = (await response.json()) as CreateRoomResponse;
      enterRoom(created.room_code, created.websocket_url);
    } catch {
      setError(`The Rust server at ${apiBase()} is not reachable.`);
    } finally {
      setCreating(false);
    }
  };

  if (view === "room") {
    return (
      <GameRoom
        avatar={avatar}
        mobilePanel={mobilePanel}
        name={name.trim()}
        onClosePanel={() => setMobilePanel(null)}
        onExit={() => {
          window.history.pushState({}, "", "/");
          setMobilePanel(null);
          setActiveRoom("");
          setActiveSocket("");
          setView("landing");
        }}
        onPanel={setMobilePanel}
        roomCode={activeRoom}
        websocketUrl={activeSocket}
      />
    );
  }

  return (
    <main className={styles.landing}>
      <div className={styles.backgroundDoodles} aria-hidden="true">
        <span className={styles.doodleOne}>✦</span><span className={styles.doodleTwo}>⌁</span><span className={styles.doodleThree}>○</span>
      </div>
      <nav className={styles.landingNav} aria-label="Main navigation">
        <Brand />
        <div className={styles.navNote}><Lightning size={16} weight="fill" /> Rust-powered rooms</div>
      </nav>

      <div className={styles.heroGrid}>
        <section className={styles.heroCopy}>
          <span className={styles.kicker}><Sparkle size={15} weight="fill" /> No account. No install. Just draw.</span>
          <h1 className="display-font">Your friends are<br />terrible artists.<br /><em>Prove it.</em></h1>
          <p>A fast drawing game with a serious canvas, tiny invite codes, and no clutter between you and the chaos.</p>
          <div className={styles.heroProof}>
            <div className={styles.avatarStack}>
              {HERO_AVATARS.map((player) => <Avatar key={player.name} name={player.name} size={38} value={player.avatar} />)}
            </div>
            <span><b>Room-ready in seconds</b><small>Private by default · up to 12 players</small></span>
          </div>
        </section>

        <section className={styles.joinCard}>
          <div className={styles.modeTabs}>
            <button data-active={mode === "create"} onClick={() => { setMode("create"); setError(""); }} type="button">Create room</button>
            <button data-active={mode === "join"} onClick={() => { setMode("join"); setError(""); }} type="button">Join room</button>
          </div>
          <form onSubmit={submit}>
            <label className={styles.fieldLabel}>
              <span>Your name</span>
              <div className={styles.nameField}>
                <Avatar name={name} size={42} value={avatar} />
                <input autoComplete="nickname" maxLength={24} onChange={(event) => setName(event.target.value)} placeholder="Call me…" value={name} />
                <span>{name.length}/24</span>
              </div>
            </label>
            {mode === "join" ? (
              <label className={styles.fieldLabel}>
                <span>Room code</span>
                <input autoCapitalize="characters" className={styles.codeInput} maxLength={6} onChange={(event) => setRoomCode(event.target.value.toUpperCase())} placeholder="PAINT7" value={roomCode} />
              </label>
            ) : (
              <>
                <fieldset className={styles.roundPicker}>
                  <legend>Number of rounds</legend>
                  <div>
                    {ROUND_OPTIONS.map((option) => (
                      <button aria-pressed={rounds === option} data-active={rounds === option} key={option} onClick={() => setRounds(option)} type="button">
                        <b>{option}</b><span>rounds</span>
                      </button>
                    ))}
                  </div>
                </fieldset>
                <div className={styles.roomPromise}>
                  <LockKey size={20} weight="duotone" />
                  <span><b>A private room, instantly</b><small>You’ll get a six-character invite code.</small></span>
                </div>
              </>
            )}
            {error ? <p className={styles.formError} role="alert">{error}</p> : null}
            <button className={styles.primaryButton} disabled={creating} type="submit">
              {creating ? "Warming up the room…" : mode === "create" ? "Create a room" : "Join the room"}
              {!creating ? <ArrowRight size={19} weight="bold" /> : null}
            </button>
          </form>
          <AvatarPicker name={name} onChange={setAvatar} value={avatar} />
        </section>
      </div>

      <section className={styles.demoStrip}>
        <div><span className={styles.eyebrow}>The good stuff</span><h2 className="display-font">A real canvas. Not a toy.</h2></div>
        <div className={styles.featureList}>
          <span><PaintBucket size={18} weight="duotone" /> Shapes + rich brushes</span>
          <span><GameController size={18} weight="duotone" /> Mobile-first</span>
          <span><Lightning size={18} weight="duotone" /> Binary WebSocket</span>
        </div>
      </section>
    </main>
  );
}

function Brand() {
  return <div className={styles.brand}><span className={styles.brandMark}><span /><span /><span /></span><b className="display-font">NDraw</b></div>;
}

function useCountdown(deadlineUnixMs: number | null | undefined): string {
  const [now, setNow] = useState(0);
  useEffect(() => {
    const update = () => setNow(Date.now());
    const first = window.setTimeout(update, 0);
    const interval = window.setInterval(update, 250);
    return () => { window.clearTimeout(first); window.clearInterval(interval); };
  }, []);
  if (!deadlineUnixMs || now === 0) return "--:--";
  const totalSeconds = Math.max(0, Math.ceil((deadlineUnixMs - now) / 1000));
  return `${Math.floor(totalSeconds / 60).toString().padStart(2, "0")}:${(totalSeconds % 60).toString().padStart(2, "0")}`;
}

function chatLinesFromSnapshot(message: Extract<ServerMessage, { kind: "welcome" | "resume" }>): ChatLine[] {
  return message.snapshot.chatHistory.map((event, index) => ({
    id: index + 1,
    kind: event.kind,
    playerId: event.playerId,
    text: event.text,
  }));
}

function roomHeader(
  phase: PhaseView | null,
  selfPlayerId: number | null,
  drawerName: string | undefined,
  secretWord: string | null,
): { label: string; value: string } {
  if (!phase) return { label: "Joining room", value: "Connecting…" };
  if (phase.phase === "lobby") return { label: "Room lobby", value: "Invite your friends" };
  if (phase.phase === "choosingWord") return phase.drawer === selfPlayerId
    ? { label: "Your turn", value: "Choose a word" }
    : { label: "Up next", value: `${drawerName ?? "The drawer"} is choosing` };
  if (phase.phase === "drawing") return phase.drawer === selfPlayerId
    ? { label: "You’re drawing", value: secretWord ?? "Your word" }
    : { label: `${drawerName ?? "Someone"} is drawing`, value: phase.maskedWord ?? "Guess the word" };
  if (phase.phase === "roundEnd") return { label: "The word was", value: phase.maskedWord ?? "Round complete" };
  return { label: "Game over", value: "Final scores" };
}

function GameRoom({
  avatar,
  mobilePanel,
  name,
  onClosePanel,
  onExit,
  onPanel,
  roomCode,
  websocketUrl,
}: {
  avatar: AvatarBytes;
  mobilePanel: MobilePanel;
  name: string;
  onClosePanel: () => void;
  onExit: () => void;
  onPanel: (panel: MobilePanel) => void;
  roomCode: string;
  websocketUrl: string;
}) {
  const clientRef = useRef<NDrawClient | null>(null);
  const lineIdRef = useRef(1_000);
  const [room, dispatch] = useReducer(reduceRoom, roomCode, initialRoomState);
  const [connection, setConnection] = useState<ConnectionState>("connecting");
  const [serverNotice, setServerNotice] = useState("Connecting to the Rust room…");
  const [copied, setCopied] = useState(false);
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [draft, setDraft] = useState("");
  const phase = room.phase;
  const self = roomPlayer(room, room.selfPlayerId);
  const drawer = roomPlayer(room, phase?.drawer ?? null);
  const isDrawer = phase?.phase === "drawing" && phase.drawer === room.selfPlayerId;
  const canGuess = phase?.phase === "drawing" && !isDrawer && !self?.hasGuessed;
  const timer = useCountdown(phase?.deadlineUnixMs);

  useEffect(() => {
    const addLine = (line: Omit<ChatLine, "id">) => {
      lineIdRef.current += 1;
      setLines((current) => [...current, { ...line, id: lineIdRef.current }].slice(-80));
    };
    const client = new NDrawClient(
      (message) => {
        dispatch(message);
        if (message.kind === "welcome" || message.kind === "resume") {
          setLines(chatLinesFromSnapshot(message));
          setServerNotice(message.kind === "welcome" ? `Connected to ${message.kind === "welcome" ? message.roomCode : roomCode}` : "Reconnected — room state restored");
        } else if (message.kind === "chat") {
          addLine({ kind: message.event.kind, playerId: message.event.playerId, text: message.event.text });
        } else if (message.kind === "guessResult") {
          if (typeof message.outcome === "object") addLine({ kind: "correct", playerId: message.playerId, text: `guessed correctly (+${message.outcome.correct})` });
          else if (message.outcome === "close") addLine({ kind: "system", playerId: message.playerId, text: "Very close!" });
        } else if (message.kind === "error") {
          setServerNotice(message.message);
          addLine({ kind: "system", playerId: null, text: message.message });
        } else if (message.kind === "bye") {
          setServerNotice("The server closed this room connection");
        }
      },
      (state) => {
        setConnection(state);
        if (state === "closed") setServerNotice("Disconnected from the room");
      },
    );
    clientRef.current = client;
    client.connect(websocketUrl, getClientToken(), name, avatar);
    return () => {
      clientRef.current = null;
      client.close();
    };
  }, [avatar, name, roomCode, websocketUrl]);

  const copyCode = async () => {
    const success = await copyText(roomCode);
    setCopied(success);
    setServerNotice(success ? `Room code ${roomCode} copied` : `Could not copy automatically — room code is ${roomCode}`);
    if (success) window.setTimeout(() => setCopied(false), 1_600);
  };

  const shareRoom = async () => {
    const url = `${window.location.origin}${roomPath(roomCode)}`;
    if (navigator.share) {
      try {
        await navigator.share({
          title: `Join my NDraw room ${roomCode}`,
          text: "Come draw with me on NDraw.",
          url,
        });
        setServerNotice("Room link shared");
        return;
      } catch (shareError) {
        if (shareError instanceof DOMException && shareError.name === "AbortError") return;
      }
    }

    const success = await copyText(url);
    setCopied(success);
    setServerNotice(success ? "Room link copied" : `Share this link: ${url}`);
    if (success) window.setTimeout(() => setCopied(false), 1_600);
  };

  const sendText = (event: FormEvent) => {
    event.preventDefault();
    const text = draft.trim();
    if (!text || connection !== "open") return;
    let sent = false;
    if (canGuess) {
      sent = Boolean(clientRef.current?.send({ kind: "guess", text }));
    } else {
      sent = Boolean(clientRef.current?.send({ kind: "chat", text }));
    }
    if (sent) setDraft("");
  };

  const sendDraw = (operation: DrawOp) => {
    clientRef.current?.send({ kind: "draw", operation });
  };

  const header = roomHeader(phase, room.selfPlayerId, drawer?.profile.displayName, room.secretWord);

  const overlay = room.ready ? (
    <PhaseOverlay
      connection={connection}
      isHost={Boolean(self?.isHost)}
      onPickWord={(choice) => clientRef.current?.send({ kind: "pickWord", choice })}
      onRematch={() => clientRef.current?.send({ kind: "rematch" })}
      onShare={shareRoom}
      onStart={() => clientRef.current?.send({ kind: "startGame" })}
      phase={phase}
      lobbyTimer={timer}
      playerCount={room.players.filter((player) => player.connected).length}
      wordOptions={room.wordOptions?.words ?? []}
      players={room.players}
      result={room.turnResult}
      rating={room.drawingRating}
      selfPlayerId={room.selfPlayerId}
      drawSeconds={room.settings?.drawSeconds ?? 100}
      onVote={(vote) => {
        const turnId = room.turnResult?.turnId;
        if (turnId !== undefined) clientRef.current?.send({ kind: "voteDrawing", turnId, vote });
      }}
    />
  ) : <PhaseOverlay connection={connection} drawSeconds={100} isHost={false} lobbyTimer="--:--" onPickWord={() => undefined} onRematch={() => undefined} onShare={() => undefined} onStart={() => undefined} onVote={() => undefined} phase={null} playerCount={0} players={[]} rating={null} result={null} selfPlayerId={null} wordOptions={[]} />;

  return (
    <main className={styles.gameShell} data-mobile-panel={mobilePanel ?? "none"}>
      <header className={styles.gameHeader}>
        <button aria-label="Leave room" className={styles.brandButton} onClick={onExit} type="button"><Brand /></button>
        <button className={styles.roomCode} onClick={copyCode} type="button"><span>Room</span><b>{roomCode}</b>{copied ? <Check size={16} weight="bold" /> : <Copy size={16} weight="bold" />}</button>
        <div className={styles.turnPrompt}><span>{header.label}</span><strong className="display-font">{header.value}</strong></div>
        <div className={styles.roundInfo}><span>Round <b>{phase?.round ?? 0} / {phase?.totalRounds ?? room.settings?.rounds ?? 3}</b></span><strong className="display-font">{timer}</strong></div>
      </header>

      <div className={styles.gameGrid}>
        <PlayerRail copied={copied} drawerId={phase?.drawer ?? null} maxPlayers={room.settings?.maxPlayers ?? 12} onCopy={copyCode} players={room.players} roomCode={roomCode} selfPlayerId={room.selfPlayerId} />
        <section className={styles.canvasColumn}>
          <div className={styles.canvasStatus}>
            <span><span className={styles.liveDot} data-state={connection} />{connection === "open" ? "Live room" : connection === "connecting" ? "Connecting" : "Offline"}</span>
            <small>{room.lastError ?? serverNotice}</small>
          </div>
          <div className={styles.roomCats} aria-hidden="true">
            <span className={styles.roomCatOne}><i>mrrp!</i><Cat size={36} weight="duotone" /></span>
            <span className={styles.roomCatTwo}><Cat size={31} weight="fill" /></span>
          </div>
          <DrawingStudio actions={room.canvasActions} enabled={connection === "open" && isDrawer} onDraw={isDrawer ? sendDraw : undefined} showTools={isDrawer} />
          {overlay}
        </section>
        <ChatPanel canSend={connection === "open" && room.ready} draft={draft} lines={lines} mode={canGuess ? "guess" : "chat"} onDraft={setDraft} onSubmit={sendText} players={room.players} />
      </div>

      <nav className={styles.mobileNav} aria-label="Room panels">
        <button onClick={() => onPanel("players")} type="button"><UsersThree size={22} weight="bold" /><span>Players</span></button>
        <span className={styles.mobileTimer}>{timer}</span>
        <button onClick={() => onPanel("chat")} type="button"><ChatCircleDots size={22} weight="bold" /><span>Chat</span>{lines.length > 0 ? <i>{Math.min(99, lines.length)}</i> : null}</button>
      </nav>

      {mobilePanel ? (
        <div className={styles.mobileSheetBackdrop} role="presentation">
          <section className={styles.mobileSheet}>
            <div className={styles.mobileSheetHeader}><strong>{mobilePanel === "players" ? "Players" : "Chat & guesses"}</strong><button aria-label="Close panel" className="icon-button" onClick={onClosePanel} type="button"><X size={18} weight="bold" /></button></div>
            {mobilePanel === "players"
              ? <PlayerRail copied={copied} drawerId={phase?.drawer ?? null} maxPlayers={room.settings?.maxPlayers ?? 12} mobile onCopy={copyCode} players={room.players} roomCode={roomCode} selfPlayerId={room.selfPlayerId} />
              : <ChatPanel canSend={connection === "open" && room.ready} draft={draft} lines={lines} mobile mode={canGuess ? "guess" : "chat"} onDraft={setDraft} onSubmit={sendText} players={room.players} />}
          </section>
        </div>
      ) : null}
    </main>
  );
}

function PhaseOverlay({ connection, drawSeconds, isHost, lobbyTimer, onPickWord, onRematch, onShare, onStart, onVote, phase, playerCount, players, rating, result, selfPlayerId, wordOptions }: {
  connection: ConnectionState;
  drawSeconds: number;
  isHost: boolean;
  lobbyTimer: string;
  onPickWord: (choice: number) => void;
  onRematch: () => void;
  onShare: () => void;
  onStart: () => void;
  onVote: (vote: DrawingVote | null) => void;
  phase: ReturnType<typeof initialRoomState>["phase"];
  playerCount: number;
  players: PlayerView[];
  rating: ReturnType<typeof initialRoomState>["drawingRating"];
  result: ReturnType<typeof initialRoomState>["turnResult"];
  selfPlayerId: number | null;
  wordOptions: string[];
}) {
  if (!phase) return <div className={styles.phaseOverlay}><span className={styles.phaseSpinner} /><h2 className="display-font">Joining the room…</h2><p>Waiting for the authoritative room snapshot.</p></div>;
  if (phase.phase === "drawing") return null;
  if (phase.phase === "lobby") return (
    <div className={styles.phaseOverlay}>
      <span className={styles.phaseBadge}>{playerCount} player{playerCount === 1 ? "" : "s"} connected</span>
      <h2 className="display-font">The room is ready</h2>
      <p>{playerCount < 2 ? "Share the room code. Two players are required to begin." : isHost ? "Everyone’s here. Start when you’re ready." : "Waiting for the host to start."}</p>
      <div className={styles.lobbyActions}>
        {isHost ? <button className={styles.primaryButton} disabled={connection !== "open" || playerCount < 2} onClick={onStart} type="button">Start game <ArrowRight size={18} weight="bold" /></button> : null}
        <button className={styles.shareRoomButton} disabled={connection !== "open"} onClick={onShare} type="button"><ShareNetwork size={18} weight="bold" /> Share room link</button>
      </div>
      <p className={styles.lobbyExpiry}>Room held for <strong>{lobbyTimer}</strong> only — start before it expires.</p>
      <div className={styles.lobbyJoinRule}><LockKey size={16} weight="duotone" /><span>Once the game starts, new players cannot join. Players already inside can always reconnect.</span></div>
    </div>
  );
  if (phase.phase === "choosingWord") return wordOptions.length > 0 ? (
    <div className={styles.phaseOverlay}>
      <span className={styles.phaseBadge}>Your turn to draw</span>
      <h2 className="display-font">Choose a word</h2>
      <div className={styles.wordChoices}>{wordOptions.map((word, index) => <button key={`${word}-${index}`} onClick={() => onPickWord(index)} type="button">{word}</button>)}</div>
    </div>
  ) : <div className={styles.phaseOverlay}><span className={styles.phaseSpinner} /><h2 className="display-font">The drawer is choosing…</h2><p>The round will begin in a moment.</p></div>;
  if (phase.phase === "roundEnd") {
    const awards = result?.awards ?? [];
    const viewerIsDrawer = phase.drawer === selfPlayerId;
    const vote = rating?.viewerVote ?? null;
    return (
      <div className={`${styles.phaseOverlay} ${styles.resultOverlay}`}>
        <span className={styles.phaseBadge}>Drawing complete</span>
        <div className={styles.resultTitle}><small>The word was</small><h2 className="display-font">{result?.word ?? phase.maskedWord}</h2></div>
        <div className={styles.awardList}>
          {awards.length === 0 ? <span className={styles.noAwards}>No points earned this turn</span> : awards.map((award) => {
            const player = players.find((candidate) => candidate.playerId === award.playerId);
            if (!player) return null;
            return (
              <div className={styles.awardRow} key={`${result?.turnId}-${award.playerId}`}>
                <Avatar name={player.profile.displayName} size={31} value={player.profile.avatar} />
                <span><b>{player.profile.displayName}{player.playerId === selfPlayerId ? " (you)" : ""}</b><small>{award.reason === "correctGuess" ? "Correct guess" : "Drawer bonus"}</small></span>
                <strong>+{award.points}</strong>
              </div>
            );
          })}
        </div>
        <div className={styles.ratingRow}>
          <span>{viewerIsDrawer ? "Your drawing’s rating" : "Rate the drawing"}</span>
          <button aria-label="Like drawing" data-active={vote === "like"} disabled={viewerIsDrawer || connection !== "open"} onClick={() => onVote(vote === "like" ? null : "like")} type="button"><ThumbsUp size={18} weight={vote === "like" ? "fill" : "bold"} /> {rating?.likes ?? 0}</button>
          <button aria-label="Dislike drawing" data-active={vote === "dislike"} disabled={viewerIsDrawer || connection !== "open"} onClick={() => onVote(vote === "dislike" ? null : "dislike")} type="button"><ThumbsDown size={18} weight={vote === "dislike" ? "fill" : "bold"} /> {rating?.dislikes ?? 0}</button>
        </div>
        <p className={styles.scoreExplanation}>Fast correct guesses earn 50–500 points over {drawSeconds}s. The drawer earns one quarter of all guesser points. Ratings never affect scores.</p>
      </div>
    );
  }
  const leaderboard = players
    .map((player, joinedIndex) => ({ player, joinedIndex }))
    .sort((left, right) => right.player.score - left.player.score || left.joinedIndex - right.joinedIndex);
  return (
    <div className={`${styles.phaseOverlay} ${styles.leaderboardOverlay}`}>
      <span className={styles.phaseBadge}>Game over</span>
      <h2 className="display-font">Final leaderboard</h2>
      <ol className={styles.leaderboard}>
        {leaderboard.map(({ player }, index) => (
          <li data-winner={index === 0} key={player.playerId}>
            <span className={styles.leaderboardRank}>{index === 0 ? <CrownSimple size={17} weight="fill" /> : index + 1}</span>
            <Avatar name={player.profile.displayName} size={38} value={player.profile.avatar} />
            <span className={styles.leaderboardName}><b>{player.profile.displayName}{player.playerId === selfPlayerId ? " (you)" : ""}</b><small>{index === 0 ? "Room champion" : player.connected ? "Final score" : "Disconnected"}</small></span>
            <strong>{player.score}<small> pts</small></strong>
          </li>
        ))}
      </ol>
      <p>{isHost ? "Same room, same settings, fresh scores." : "Waiting for the host to start a rematch."}</p>
      {isHost ? <button className={styles.primaryButton} disabled={connection !== "open"} onClick={onRematch} type="button">Play again <ArrowRight size={18} weight="bold" /></button> : null}
    </div>
  );
}

function PlayerRail({ copied, drawerId, maxPlayers, onCopy, players, roomCode, selfPlayerId, mobile = false }: {
  copied: boolean;
  drawerId: number | null;
  maxPlayers: number;
  onCopy: () => void;
  players: PlayerView[];
  roomCode: string;
  selfPlayerId: number | null;
  mobile?: boolean;
}) {
  const rankedPlayers = rankPlayers(players);
  return (
    <aside className={`${styles.playerRail} ${mobile ? styles.mobilePanelContent : ""}`}>
      <div className={styles.panelHeading}><span>Players</span><small>{players.filter((player) => player.connected).length} / {maxPlayers}</small></div>
      <ol className={styles.playerList}>
        {rankedPlayers.map((player, index) => {
          const status = !player.connected ? "offline" : player.playerId === drawerId ? "drawing" : player.hasGuessed ? "guessed" : "idle";
          return (
            <li data-status={status} key={player.playerId}>
              <span className={styles.rank}>{index + 1}</span>
              <div className={styles.playerAvatar}><Avatar name={player.profile.displayName} size={43} value={player.profile.avatar} />{player.isHost ? <CrownSimple size={14} weight="fill" /> : null}</div>
              <span className={styles.playerName}><b>{player.profile.displayName}{player.playerId === selfPlayerId ? " (you)" : ""}</b><small>{status === "drawing" ? "drawing now" : status === "guessed" ? "guessed it" : status === "offline" ? "disconnected" : player.isHost ? "host" : "thinking…"}</small></span>
              <strong>{player.score}</strong>
            </li>
          );
        })}
      </ol>
      <div className={styles.scoreRules}><Lightning size={15} weight="duotone" /><span><b>How points work</b><small>Guess faster for 50–500. Drawer gets one quarter of the guessers’ points.</small></span></div>
      <div className={styles.inviteCard}><LinkSimple size={20} weight="duotone" /><span><small>Invite code</small><b>{roomCode}</b></span><button aria-label="Copy room code" onClick={onCopy} type="button">{copied ? <Check size={18} weight="bold" /> : <Copy size={18} weight="bold" />}</button></div>
    </aside>
  );
}

function ChatPanel({ canSend, draft, lines, mode, onDraft, onSubmit, players, mobile = false }: {
  canSend: boolean;
  draft: string;
  lines: ChatLine[];
  mode: "guess" | "chat";
  onDraft: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
  players: PlayerView[];
  mobile?: boolean;
}) {
  const messagesRef = useRef<HTMLDivElement>(null);
  const playerName = (playerId: number | null) => players.find((player) => player.playerId === playerId)?.profile.displayName ?? "Game";

  useEffect(() => {
    const messages = messagesRef.current;
    if (messages) messages.scrollTop = messages.scrollHeight;
  }, [lines]);

  return (
    <aside className={`${styles.chatPanel} ${mobile ? styles.mobilePanelContent : ""}`}>
      <div className={styles.panelHeading}><span>Chat & guesses</span><span className={styles.onlinePill}>{canSend ? "Live" : "Offline"}</span></div>
      <div className={styles.chatMessages} aria-live="polite" ref={messagesRef}>
        {lines.length === 0 ? <div className={styles.chatHint}>Nothing here yet. Say hello.</div> : null}
        {lines.map((line) => line.kind === "correct" ? (
          <div className={styles.correctMessage} key={line.id}><Check size={15} weight="bold" /> {playerName(line.playerId)} {line.text}</div>
        ) : line.kind === "system" ? (
          <div className={styles.chatHint} key={line.id}>{line.text}</div>
        ) : (
          <p data-kind={line.kind} key={line.id}><b>{playerName(line.playerId)}{line.kind === "guess" ? " guessed" : ""}</b><span>{line.text}</span></p>
        ))}
      </div>
      <form className={styles.chatForm} onSubmit={onSubmit}>
        <input
          aria-disabled={!canSend}
          autoComplete="off"
          enterKeyHint="send"
          inputMode="text"
          maxLength={200}
          onChange={(event) => onDraft(event.target.value)}
          placeholder={canSend ? mode === "guess" ? "Type a guess…" : "Send a message…" : "Reconnect to send…"}
          value={draft}
        />
        <button aria-label={mode === "guess" ? "Send guess" : "Send message"} disabled={!canSend} type="submit"><PaperPlaneTilt size={18} weight="fill" /></button>
      </form>
    </aside>
  );
}
