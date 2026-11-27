import { notFound } from "next/navigation";
import GameManage from "@/components/games/GameManage";
import { isGameId } from "@/lib/games-api";

export default async function ManageGamePage({ params }: { params: Promise<{ gameId: string }> }) {
  const { gameId } = await params;
  if (!isGameId(gameId)) notFound();
  return <GameManage gameId={gameId} />;
}
