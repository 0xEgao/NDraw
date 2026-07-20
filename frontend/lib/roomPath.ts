const ROOM_CODE_PATTERN = /^[A-HJ-NP-Z2-9]{6}$/;

/** Returns a canonical room code, or `null` when the value is not a room code. */
export function normalizeRoomCode(value: string): string | null {
  const normalized = value.trim().toUpperCase();
  return ROOM_CODE_PATTERN.test(normalized) ? normalized : null;
}

/** Builds the public, reload-safe browser path for a room. */
export function roomPath(roomCode: string): string {
  const normalized = normalizeRoomCode(roomCode);
  return normalized ? `/${normalized}` : "/";
}

/** Reads a room code from a root-level room path such as `/DJBUKN`. */
export function roomCodeFromPath(pathname: string): string | null {
  const segments = pathname.split("/").filter(Boolean);
  return segments.length === 1 ? normalizeRoomCode(segments[0]) : null;
}
