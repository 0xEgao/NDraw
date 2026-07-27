"use client";

import { Microphone, MicrophoneSlash, SpinnerGap, WarningCircle } from "@phosphor-icons/react";
import type { Room as LiveKitRoom } from "livekit-client";
import { useEffect, useRef, useState } from "react";
import styles from "./ndraw.module.css";

type VoiceState = "off" | "connecting" | "muted" | "live" | "error";

interface TokenResponse {
  token: string;
  url: string;
}

export function VoiceControl({
  apiBase,
  displayName,
  roomCode,
}: {
  apiBase: string;
  displayName: string;
  roomCode: string;
}) {
  const [voiceState, setVoiceState] = useState<VoiceState>("off");
  const [error, setError] = useState("");
  const roomRef = useRef<LiveKitRoom | null>(null);
  const audioRootRef = useRef<HTMLSpanElement | null>(null);
  const mountedRef = useRef(true);

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
    setVoiceState("connecting");
    setError("");
    try {
      const params = new URLSearchParams({
        room_code: roomCode,
        display_name: displayName,
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
      room.on(RoomEvent.Disconnected, () => {
        roomRef.current = null;
        audioRootRef.current?.replaceChildren();
        if (mountedRef.current) setVoiceState("off");
      });

      await room.connect(credentials.url, credentials.token);
      if (!mountedRef.current) {
        await room.disconnect();
        return;
      }
      roomRef.current = room;
      await room.startAudio();
      await room.localParticipant.setMicrophoneEnabled(true);
      setVoiceState("live");
    } catch (cause) {
      const room = roomRef.current;
      roomRef.current = null;
      if (room) await room.disconnect();
      if (!mountedRef.current) return;
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
      setVoiceState(enable ? "live" : "muted");
      setError("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Microphone access failed");
      setVoiceState("error");
    }
  };

  const label = voiceState === "live"
    ? "Mute microphone"
    : voiceState === "muted"
      ? "Unmute microphone"
      : voiceState === "connecting"
        ? "Connecting voice chat"
        : voiceState === "error"
          ? "Retry voice chat"
          : "Join voice chat";
  const icon = voiceState === "connecting"
    ? <SpinnerGap size={17} weight="bold" />
    : voiceState === "live"
      ? <Microphone size={17} weight="fill" />
      : voiceState === "error"
        ? <WarningCircle size={17} weight="fill" />
        : <MicrophoneSlash size={17} weight="fill" />;

  return (
    <>
      <button
        aria-label={label}
        aria-pressed={voiceState === "live"}
        className={styles.voiceButton}
        data-state={voiceState}
        onClick={toggle}
        title={error || label}
        type="button"
      >
        {icon}
        <span>{voiceState === "live" ? "Live" : voiceState === "muted" ? "Muted" : "Voice"}</span>
      </button>
      <span aria-hidden="true" className={styles.remoteAudio} ref={audioRootRef} />
    </>
  );
}
