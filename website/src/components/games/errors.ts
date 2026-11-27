import { GamesApiError } from "@/lib/games-api";

/** The plain sentence for a failed Games action: the service's own words when it gave them. */
export function GameApiErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof GamesApiError) return error.message;
  if (error instanceof Error && error.message) return error.message;
  return fallback;
}
