import { describe, expect, it } from "vitest";
import {
  conceptGroups,
  getWalletLockedInitialAction,
  getWalletSetupHref,
  parseWalletSetupMode,
  supportedPaths,
} from "@/lib/ultraverse";

describe("UltraVerse directory data", () => {
  it("keeps the three supported destinations explicit", () => {
    expect(supportedPaths.map(({ id, href }) => [id, href])).toEqual([
      ["wallet", "/send-ultra"],
      ["appchains", "/login?next=%2Fappchains"],
      ["validator", "/validator"],
    ]);
  });

  it("keeps all concepts grouped without turning them into services", () => {
    expect(conceptGroups.flatMap(({ items }) => items)).toHaveLength(12);
  });

  it("describes AppChains as owner-signed, and this site as read-only for anchoring", () => {
    const appchains = supportedPaths.find(({ id }) => id === "appchains");
    expect(appchains).toBeDefined();
    const facts = (appchains?.facts ?? []).join(" ");

    // Identity by id, one L1 owner, signing in the wallet: the model the node actually enforces.
    expect(facts).toMatch(/chain id/i);
    expect(facts).toMatch(/owner/i);
    expect(facts).toMatch(/mobile wallet/i);
    // The retired claims: a server-side anchor quest, free registration, and a treasury debit.
    expect(facts).not.toMatch(/state-commitment/);
    expect(facts).not.toMatch(/registration is free/i);
    expect(facts).not.toMatch(/funded treasury/i);
  });
});

describe("UltraVerse wallet setup links", () => {
  it.each(["create", "restore"] as const)("builds the %s setup destination", (mode) => {
    expect(getWalletSetupHref(mode)).toBe(`/send-ultra?setup=${mode}`);
    expect(parseWalletSetupMode(mode)).toBe(mode);
  });

  it("ignores unknown and repeated setup values", () => {
    expect(parseWalletSetupMode("replace")).toBeUndefined();
    expect(parseWalletSetupMode(["restore", "create"])).toBeUndefined();
    expect(parseWalletSetupMode(undefined)).toBeUndefined();
  });

  it("keeps new-wallet replacement confirmation for existing local wallets", () => {
    expect(getWalletLockedInitialAction("create")).toBe("replace-confirm");
    expect(getWalletLockedInitialAction("restore")).toBe("restore");
    expect(getWalletLockedInitialAction(undefined)).toBe("locked");
  });
});
