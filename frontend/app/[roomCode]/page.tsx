import { notFound } from "next/navigation";
import { NDrawApp } from "../../components/NDrawApp";
import { normalizeRoomCode } from "../../lib/roomPath";

export default async function RoomPage({
  params,
}: {
  params: Promise<{ roomCode: string }>;
}) {
  const { roomCode } = await params;
  const normalized = normalizeRoomCode(roomCode);
  if (!normalized) notFound();

  return <NDrawApp initialRoomCode={normalized} />;
}
