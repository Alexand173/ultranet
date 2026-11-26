import { notFound } from "next/navigation";
import GameDetail from "@/components/games/GameDetail";
import { isGameId } from "@/lib/games-api";

export default async function GamePage({ params }: { params: Promise<{ gameId: string }> }) {
  const { gameId } = await params;
  // A game id is 32 lowercase hex characters; anything else cannot name a game.
  if (!isGameId(gameId)) notFound();
  return <GameDetail gameId={gameId} />;
}
