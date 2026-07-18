"use client";

import { Avatar as DiceAvatar, Style } from "@dicebear/core";
import bigSmile from "@dicebear/styles/big-smile.json";
import Image from "next/image";
import { useMemo } from "react";

const style = new Style(bigSmile);
const BACKGROUNDS = ["ffe2d7", "e8ddff", "cceee5", "fff0ad", "d8e8ff", "ffd9e9"];

export type AvatarBytes = readonly [number, number, number, number, number, number, number, number];

export function randomAvatar(): AvatarBytes {
  const bytes = new Uint8Array(8);
  crypto.getRandomValues(bytes);
  return [...bytes] as unknown as AvatarBytes;
}

function avatarSeed(bytes: AvatarBytes): string {
  return bytes.map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function Avatar({
  value,
  name,
  size = 52,
}: {
  value: AvatarBytes;
  name: string;
  size?: number;
}) {
  const source = useMemo(() => {
    const avatar = new DiceAvatar(style, {
      seed: avatarSeed(value),
      backgroundColor: [BACKGROUNDS[value[6] % BACKGROUNDS.length]],
      borderRadius: 50,
      size: Math.max(48, size * 2),
    });
    return avatar.toDataUri();
  }, [size, value]);

  return (
    <Image
      alt={`${name || "Player"} avatar`}
      draggable={false}
      height={size}
      src={source}
      style={{ width: size, height: size }}
      unoptimized
      width={size}
    />
  );
}
