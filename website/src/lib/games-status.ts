import type { ResultStage, StoreGame } from "@/lib/games.types";

/**
 * What a visitor is told about a game and about a result, in one place so the words never drift from the rules.
 * The reason codes are the service's own; Android reads the same codes and says the same things.
 *
 * Rules this file keeps:
 * - A game is "Rewards Available" only when the service says a reward is on offer right now (an approved policy and
 *   a pool that can cover it, with payouts switched on). Supporting ULTRA is not a promise.
 * - A reserved reward is not a paid reward. Only the network-confirmed stage reads "Paid".
 */

export type AvailabilityKind = "record_only" | "unavailable" | "rewards_unavailable" | "rewards_available";

export interface Availability {
  kind: AvailabilityKind;
  /** The badge text. */
  label: string;
  /** Why, in a sentence. Present whenever the kind is not "rewards_available". */
  reason: string | null;
}

const PLAYABLE_REASONS: Record<string, string> = {
  game_not_active: "This game is not active right now.",
  record_only_no_approved_adapter: "Results are recorded only: no approved verifier exists for this game yet.",
  adapter_module_not_installed: "The platform has not installed this game's verifier yet.",
};

const REWARD_REASONS: Record<string, string> = {
  not_playable: "Rewards need a playable game.",
  rewards_not_enabled: "Rewards are not switched on yet.",
  no_approved_reward_policy: "This game has no approved reward policy.",
  pool_cannot_cover_the_reward: "The game's reward pool cannot cover a reward right now.",
};

const NOT_REWARDED: Record<string, string> = {
  policy_does_not_pay_this_outcome: "Verified, but this outcome does not earn a reward.",
  daily_limit_reached: "Verified, but a daily reward limit was reached.",
  rewards_not_enabled: "Verified, but rewards are not switched on yet.",
  no_approved_reward_policy: "Verified, but this game has no approved reward policy.",
  pool_cannot_cover_the_reward: "Verified, but the game's reward pool cannot cover the reward.",
};

export function playableReasonText(code: string | null): string {
  if (code === null) return "Ready to play.";
  return PLAYABLE_REASONS[code] ?? "This game cannot be played right now.";
}

export function rewardsReasonText(code: string | null): string {
  if (code === null) return "A reward is on offer.";
  return REWARD_REASONS[code] ?? "No reward is on offer right now.";
}

export function notRewardedText(code: string | null): string | null {
  if (code === null) return null;
  return NOT_REWARDED[code] ?? "Verified, but no reward was reserved.";
}

export function availabilityOf(game: StoreGame): Availability {
  if (!game.playable) {
    if (game.playableReason === "record_only_no_approved_adapter" || game.validationScheme === "record_only") {
      return { kind: "record_only", label: "Record Only", reason: playableReasonText("record_only_no_approved_adapter") };
    }
    return { kind: "unavailable", label: "Unavailable", reason: playableReasonText(game.playableReason) };
  }
  if (game.rewards === "available") {
    return { kind: "rewards_available", label: "Rewards Available", reason: null };
  }
  return { kind: "rewards_unavailable", label: "Rewards Unavailable", reason: rewardsReasonText(game.rewardsReason) };
}

/** The whole story of a result in one phrase. Unknown stages are never described as paid. */
export function stageLabel(stage: string): string {
  switch (stage as ResultStage) {
    case "verified":
      return "Verified — No Reward";
    case "reward_reserved":
      return "Reward Reserved — Not Yet Paid";
    case "reward_paid":
      return "Paid — Confirmed on UltraNet";
    case "reward_released":
      return "Reward Released — Not Paid";
    default:
      return "Unknown State";
  }
}

export function isPaid(stage: string): boolean {
  return stage === "reward_paid";
}

export function payoutStateText(state: string | null): string | null {
  switch (state) {
    case null:
      return null;
    case "queued":
      return "Payment queued";
    case "signed":
      return "Payment signed, waiting to be sent";
    case "submitted":
      return "Payment sent, waiting for the network to confirm";
    case "confirmed":
      return "Payment confirmed by the network";
    case "failed":
      return "Payment could not be made";
    default:
      return state;
  }
}

export type GameFilter = "all" | "playable" | "rewards" | "record_only" | "mine";

export const GAME_FILTERS: ReadonlyArray<{ id: GameFilter; label: string }> = [
  { id: "all", label: "All games" },
  { id: "playable", label: "Playable" },
  { id: "rewards", label: "Rewards available" },
  { id: "record_only", label: "Record only" },
  { id: "mine", label: "On my AppChains" },
];

/** Search is by name, description or id; identity is the id. Filters use only facts the service reported. */
export function filterGames(games: readonly StoreGame[], query: string, filter: GameFilter, wallet: string | null): StoreGame[] {
  const needle = query.trim().toLowerCase();
  return games.filter((game) => {
    if (needle && !(`${game.name} ${game.description} ${game.gameId} ${game.chainId}`.toLowerCase().includes(needle))) return false;
    switch (filter) {
      case "playable":
        return game.playable;
      case "rewards":
        return game.rewards === "available";
      case "record_only":
        return availabilityOf(game).kind === "record_only";
      case "mine":
        return wallet !== null && game.owner === wallet;
      default:
        return true;
    }
  });
}

/** Featured means something checkable: a playable game with a reward on offer right now. No popularity is implied. */
export function featuredGames(games: readonly StoreGame[], limit = 3): StoreGame[] {
  return games.filter((game) => game.playable && game.rewards === "available").slice(0, limit);
}

/** Base units (six decimals) to a plain ULTRA string without inventing precision. */
export function formatUltra(baseUnits: number | null): string {
  if (baseUnits === null || !Number.isSafeInteger(baseUnits) || baseUnits < 0) return "—";
  const whole = Math.floor(baseUnits / 1_000_000);
  const fraction = baseUnits % 1_000_000;
  const grouped = whole.toLocaleString("en-US");
  return fraction === 0 ? `${grouped} ULTRA` : `${grouped}.${String(fraction).padStart(6, "0").replace(/0+$/, "")} ULTRA`;
}

/** ULTRA with up to six decimals into base units; null when it is not a positive amount that fits. */
export function parseUltraUnits(text: string): number | null {
  const trimmed = text.trim();
  if (!/^[0-9]{1,12}(\.[0-9]{1,6})?$/.test(trimmed)) return null;
  const [whole, fraction = ""] = trimmed.split(".");
  const units = Number(whole) * 1_000_000 + Number(fraction.padEnd(6, "0"));
  return Number.isSafeInteger(units) && units > 0 ? units : null;
}

export function shortId(value: string | null, head = 8, tail = 6): string {
  if (!value) return "—";
  return value.length <= head + tail + 1 ? value : `${value.slice(0, head)}…${value.slice(-tail)}`;
}
