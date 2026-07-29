"use client";

import {
  Microphone,
  MicrophoneSlash,
  SpeakerHigh,
  SpeakerLow,
  SpeakerSlash,
  SpinnerGap,
  WarningCircle,
} from "@phosphor-icons/react";
import type { Participant, Room as LiveKitRoom } from "livekit-client";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import styles from "./ndraw.module.css";

type VoiceState = "off" | "connecting" | "muted" | "live" | "error";

type ParticipantVoiceState = {
  connected: boolean;
  muted: boolean;
  speaking: boolean;
};

interface TokenResponse {
  token: string;
  url: string;
}

interface VoiceContextValue {
  error: string;
  participants: Record<number, ParticipantVoiceState>;
  selfPlayerId: number | null;
  state: VoiceState;
  toggle: () => Promise<void>;
}

const VoiceContext = createContext<VoiceContextValue | null>(null);

function playerIdFromIdentity(identity: string): number | null {
  const match = /^player-(\d+)-/.exec(identity);
  if (!match) return null;
  const playerId = Number(match[1]);
  return Number.isSafeInteger(playerId) ? playerId : null;
}

function collectParticipantStates(
  room: LiveKitRoom,
  speakingIdentities: ReadonlySet<string>,
): Record<number, ParticipantVoiceState> {
  const participants: Participant[] = [
    room.localParticipant,
    ...room.remoteParticipants.values(),
  ];
  return participants.reduce<Record<number, ParticipantVoiceState>>((states, participant) => {
    const playerId = playerIdFromIdentity(participant.identity);
    if (playerId === null) return states;
    const existing = states[playerId];
    states[playerId] = {
      connected: true,
      muted: (existing?.muted ?? true) && !participant.isMicrophoneEnabled,
      speaking: (existing?.speaking ?? false) || speakingIdentities.has(participant.identity),
    };
    return states;
  }, {});
}

export function VoiceProvider({
  apiBase,
  children,
  displayName,
  roomCode,
  selfPlayerId,
}: {
  apiBase: string;
  children: ReactNode;
  displayName: string;
  roomCode: string;
  selfPlayerId: number | null;
}) {
  const [voiceState, setVoiceState] = useState<VoiceState>("off");
  const [error, setError] = useState("");
  const [participants, setParticipants] = useState<Record<number, ParticipantVoiceState>>({});
  const roomRef = useRef<LiveKitRoom | null>(null);
  const audioRootRef = useRef<HTMLSpanElement | null>(null);
  const mountedRef = useRef(true);
  const speakingIdentitiesRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    mountedRef.current = true;
    const audioRoot = audioRootRef.current;
    return () => {
      mountedRef.current = false;
      const room = roomRef.current;
      roomRef.current = null;
      if (room) void room.disconnect();
      audioRoot?.replaceChildren();
    };
  }, []);

  const connect = async () => {
    if (selfPlayerId === null) return;
    setVoiceState("connecting");
    setError("");
    try {
      const params = new URLSearchParams({
        room_code: roomCode,
        display_name: displayName,
        player_id: selfPlayerId.toString(),
      });
      const response = await fetch(`${apiBase}/v1/voice/token?${params.toString()}`);
      if (!response.ok) {
        const message = await response.text();
        throw new Error(message || `Voice setup returned ${response.status}`);
      }
      const credentials = (await response.json()) as TokenResponse;
      const { Room, RoomEvent, Track } = await import("livekit-client");
      if (!mountedRef.current) return;

      const room = new Room({ adaptiveStream: true, dynacast: true });
      const syncParticipants = () => {
        if (!mountedRef.current) return;
        setParticipants(collectParticipantStates(room, speakingIdentitiesRef.current));
      };
      room.on(RoomEvent.TrackSubscribed, (track) => {
        if (track.kind !== Track.Kind.Audio) return;
        const element = track.attach();
        element.autoplay = true;
        element.style.display = "none";
        audioRootRef.current?.appendChild(element);
      });
      room.on(RoomEvent.TrackUnsubscribed, (track) => {
        for (const element of track.detach()) element.remove();
      });
      room.on(RoomEvent.ParticipantConnected, syncParticipants);
      room.on(RoomEvent.ParticipantDisconnected, syncParticipants);
      room.on(RoomEvent.TrackPublished, syncParticipants);
      room.on(RoomEvent.TrackUnpublished, syncParticipants);
      room.on(RoomEvent.TrackMuted, syncParticipants);
      room.on(RoomEvent.TrackUnmuted, syncParticipants);
      room.on(RoomEvent.ActiveSpeakersChanged, (speakers) => {
        speakingIdentitiesRef.current = new Set(speakers.map((speaker) => speaker.identity));
        syncParticipants();
      });
      room.on(RoomEvent.Disconnected, () => {
        roomRef.current = null;
        speakingIdentitiesRef.current.clear();
        audioRootRef.current?.replaceChildren();
        if (mountedRef.current) {
          setParticipants({});
          setVoiceState("off");
        }
      });

      await room.connect(credentials.url, credentials.token);
      if (!mountedRef.current) {
        await room.disconnect();
        return;
      }
      roomRef.current = room;
      await room.startAudio();
      await room.localParticipant.setMicrophoneEnabled(true);
      syncParticipants();
      setVoiceState("live");
    } catch (cause) {
      const room = roomRef.current;
      roomRef.current = null;
      if (room) await room.disconnect();
      if (!mountedRef.current) return;
      setParticipants({});
      setError(cause instanceof Error ? cause.message : "Voice chat could not connect");
      setVoiceState("error");
    }
  };

  const toggle = async () => {
    if (voiceState === "connecting") return;
    const room = roomRef.current;
    if (!room) {
      await connect();
      return;
    }

    const enable = voiceState !== "live";
    try {
      await room.startAudio();
      await room.localParticipant.setMicrophoneEnabled(enable);
      setParticipants(collectParticipantStates(room, speakingIdentitiesRef.current));
      setVoiceState(enable ? "live" : "muted");
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Microphone access failed");
      setVoiceState("error");
    }
  };

  const value: VoiceContextValue = {
    error,
    participants,
    selfPlayerId,
    state: voiceState,
    toggle,
  };

  return (
    <VoiceContext.Provider value={value}>
      {children}
      <span aria-hidden="true" className={styles.remoteAudio} ref={audioRootRef} />
    </VoiceContext.Provider>
  );
}

export function PlayerVoiceControl({
  displayName,
  playerId,
}: {
  displayName: string;
  playerId: number;
}) {
  const voice = useContext(VoiceContext);
  if (!voice) return null;

  const isSelf = playerId === voice.selfPlayerId;
  const participant = voice.participants[playerId];
  const participantState = participant?.speaking
    ? "speaking"
    : participant?.muted
      ? "muted"
      : participant?.connected
        ? "connected"
        : "off";

  if (!isSelf) {
    const label = participantState === "speaking"
      ? `${displayName} is speaking`
      : participantState === "muted"
        ? `${displayName} is in voice and muted`
        : participantState === "connected"
          ? `${displayName} is connected to voice`
          : `${displayName} is not connected to voice`;
    const icon = participantState === "speaking"
      ? <SpeakerHigh size={16} weight="fill" />
      : participantState === "connected"
        ? <SpeakerLow size={16} weight="fill" />
        : <SpeakerSlash size={16} weight="fill" />;

    return (
      <span
        aria-label={label}
        className={styles.playerVoiceStatus}
        data-state={participantState}
        role="img"
        title={label}
      >
        {icon}
      </span>
    );
  }

  const label = voice.state === "live"
    ? "Mute microphone"
    : voice.state === "muted"
      ? "Unmute microphone"
      : voice.state === "connecting"
        ? "Connecting voice chat"
        : voice.state === "error"
          ? "Retry voice chat"
          : "Join voice chat";
  const icon = voice.state === "connecting"
    ? <SpinnerGap size={16} weight="bold" />
    : voice.state === "live"
      ? <Microphone size={16} weight="fill" />
      : voice.state === "error"
        ? <WarningCircle size={16} weight="fill" />
        : <MicrophoneSlash size={16} weight="fill" />;

  return (
    <button
      aria-label={label}
      aria-pressed={voice.state === "live"}
      className={styles.playerVoiceButton}
      data-speaking={participantState === "speaking"}
      data-state={voice.state}
      onClick={() => void voice.toggle()}
      title={voice.error || label}
      type="button"
    >
      {icon}
    </button>
  );
}
