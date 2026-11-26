import { describe, expect, it } from "vitest";
import type { StoreGame } from "@/lib/games.types";
import {
  availabilityOf,
  featuredGames,
  filterGames,
  formatUltra,
  isPaid,
  notRewardedText,
  parseUltraUnits,
  payoutStateText,
  playableReasonText,
  rewardsReasonText,
  shortId,
  stageLabel,
} from "@/lib/games-status";

const owner = "ab".repeat(32);
const other = "cd".repeat(32);

function game(over: Partial<StoreGame> = {}): StoreGame {
  return {
    gameId: "0".repeat(31) + "1",
    chainId: 7,
    name: "Echo",
    description: "A test game",
    status: "active",
    validationScheme: "verified_adapter",
    adapterHash: "ee".repeat(32),
    owner,
    playable: true,
    playableReason: null,
    rewards: "none",
    rewardUnits: null,
    rewardsReason: "no_approved_reward_policy",
    ...over,
  };
}

describe("availability is what the service reported, in the five honest words", () => {
  it("record only when no approved verifier exists", () => {
    const a = availabilityOf(game({ playable: false, playableReason: "record_only_no_approved_adapter", validationScheme: "record_only" }));
    expect([a.kind, a.label]).toEqual(["record_only", "Record Only"]);
    expect(a.reason).toContain("recorded only");
  });

  it("rewards available only when a reward is on offer; supporting ULTRA is not a promise", () => {
    expect(availabilityOf(game({ rewards: "available", rewardUnits: 100, rewardsReason: null })).label).toBe("Rewards Available");
    const none = availabilityOf(game());
    expect([none.kind, none.label]).toEqual(["rewards_unavailable", "Rewards Unavailable"]);
    expect(none.reason).toBe("This game has no approved reward policy.");
    expect(availabilityOf(game({ rewardsReason: "pool_cannot_cover_the_reward" })).reason).toContain("pool cannot cover");
    expect(availabilityOf(game({ rewardsReason: "rewards_not_enabled" })).reason).toBe("Rewards are not switched on yet.");
  });

  it("an inactive or verifier-less game is unavailable with its own reason", () => {
    const a = availabilityOf(game({ playable: false, playableReason: "game_not_active", status: "disabled" }));
    expect([a.kind, a.reason]).toEqual(["unavailable", "This game is not active right now."]);
    expect(availabilityOf(game({ playable: false, playableReason: "adapter_module_not_installed" })).kind).toBe("unavailable");
  });

  it("every reason code has words and an unknown code never reads as good news", () => {
    for (const code of ["game_not_active", "record_only_no_approved_adapter", "adapter_module_not_installed", "x"]) expect(playableReasonText(code).length).toBeGreaterThan(5);
    for (const code of ["not_playable", "rewards_not_enabled", "no_approved_reward_policy", "pool_cannot_cover_the_reward", "x"]) expect(rewardsReasonText(code).length).toBeGreaterThan(5);
    expect(notRewardedText(null)).toBeNull();
    for (const code of ["policy_does_not_pay_this_outcome", "daily_limit_reached", "rewards_not_enabled", "x"]) expect(notRewardedText(code)).toMatch(/^Verified, but/);
  });
});

describe("a result is paid only when the network confirmed it", () => {
  it("uses the exact phrases", () => {
    expect(stageLabel("verified")).toBe("Verified — No Reward");
    expect(stageLabel("reward_reserved")).toBe("Reward Reserved — Not Yet Paid");
    expect(stageLabel("reward_paid")).toBe("Paid — Confirmed on UltraNet");
    expect(stageLabel("reward_released")).toBe("Reward Released — Not Paid");
  });

  it("nothing else, and nothing unknown, says paid", () => {
    for (const stage of ["verified", "reward_reserved", "reward_released", "", "paid", "confirmed", "reward_confirmed"]) {
      expect(stageLabel(stage).startsWith("Paid"), stage).toBe(false);
      expect(isPaid(stage), stage).toBe(false);
    }
    expect(isPaid("reward_paid")).toBe(true);
    expect(stageLabel("whatever")).toBe("Unknown State");
  });

  it("a payment in flight is described as in flight", () => {
    expect(payoutStateText("submitted")).not.toContain("confirmed by the network");
    expect(payoutStateText("signed")).toContain("waiting");
    expect(payoutStateText("confirmed")).toContain("confirmed");
    expect(payoutStateText(null)).toBeNull();
  });
});

describe("search, filters and featured use only reported facts", () => {
  const games = [
    game({ gameId: "a".repeat(32), name: "Skyline", description: "build towers", rewards: "available", rewardUnits: 100, rewardsReason: null }),
    game({ gameId: "b".repeat(32), name: "Tide", description: "record keeper", playable: false, playableReason: "record_only_no_approved_adapter", validationScheme: "record_only", owner: other }),
    game({ gameId: "c".repeat(32), name: "Echo", description: "a playable one" }),
  ];

  it("searches name, description, id and chain", () => {
    expect(filterGames(games, "sky", "all", null).map((g) => g.name)).toEqual(["Skyline"]);
    expect(filterGames(games, "RECORD", "all", null).map((g) => g.name)).toEqual(["Tide"]);
    expect(filterGames(games, "bbbb", "all", null).map((g) => g.name)).toEqual(["Tide"]);
    expect(filterGames(games, "7", "all", null)).toHaveLength(3);
    expect(filterGames(games, "nothing like this", "all", null)).toEqual([]);
  });

  it("filters by availability and by chain ownership, which needs a wallet", () => {
    expect(filterGames(games, "", "playable", null).map((g) => g.name)).toEqual(["Skyline", "Echo"]);
    expect(filterGames(games, "", "rewards", null).map((g) => g.name)).toEqual(["Skyline"]);
    expect(filterGames(games, "", "record_only", null).map((g) => g.name)).toEqual(["Tide"]);
    expect(filterGames(games, "", "mine", null)).toEqual([]);
    expect(filterGames(games, "", "mine", owner).map((g) => g.name)).toEqual(["Skyline", "Echo"]);
  });

  it("features only playable games with a reward on offer and invents no ranking", () => {
    expect(featuredGames(games).map((g) => g.name)).toEqual(["Skyline"]);
    expect(featuredGames([games[1], games[2]])).toEqual([]);
  });
});

describe("amounts", () => {
  it("formats base units without inventing precision", () => {
    expect(formatUltra(0)).toBe("0 ULTRA");
    expect(formatUltra(100)).toBe("0.0001 ULTRA");
    expect(formatUltra(1_500_000)).toBe("1.5 ULTRA");
    expect(formatUltra(1_234_567_000_000)).toBe("1,234,567 ULTRA");
    expect(formatUltra(null)).toBe("—");
    expect(formatUltra(-1)).toBe("—");
  });

  it("parses exactly and refuses what is not a positive amount", () => {
    expect(parseUltraUnits("1")).toBe(1_000_000);
    expect(parseUltraUnits("0.0001")).toBe(100);
    expect(parseUltraUnits(" 1.5 ")).toBe(1_500_000);
    expect(parseUltraUnits("0.000001")).toBe(1);
    for (const bad of ["0", "0.0000001", "-1", "1e3", "", "1,5", "abc"]) expect(parseUltraUnits(bad), bad).toBeNull();
  });

  it("shortens ids for display only", () => {
    expect(shortId("a".repeat(64))).toBe("aaaaaaaa…aaaaaa");
    expect(shortId("abc")).toBe("abc");
    expect(shortId(null)).toBe("—");
  });
});
