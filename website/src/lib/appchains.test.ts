import { describe, expect, it } from "vitest";
import { getAppChainOwnerKind, normalizeAppChainOverview } from "@/lib/appchains";

const ownerAddress = "a".repeat(64);

 describe("AppChain API normalization", () => {
  it("normalizes durable registry accounting without floating-point conversion", () => {
    const overview = normalizeAppChainOverview({
      success: true,
      chains: [
        {
          id: 2,
          name: "GameLayer",
          owner: "Ultra Labs",
          account_address: ownerAddress,
          genesis_root: "00".repeat(32),
          anchor_fee: "1000",
          balance: "999000",
          anchor_spend: "1000",
          anchor_count: "1",
          latest_anchor_at: "1785183488",
          latest_state_root: "11".repeat(32),
          anchor_availability: "test-only",
        },
        {
          id: 1,
          name: "UltraDex",
          owner: ownerAddress,
          anchor_fee: "1000",
          balance: "1000000",
          anchor_spend: "0",
          anchor_count: 0,
          anchor_availability: "not-implemented",
        },
        { id: "invalid", name: "discarded", owner: ownerAddress },
      ],
      totals: { anchor_count: "1", anchor_spend: "1000" },
      anchor_availability: "test-only",
      updated_at: "1785183488",
    });

    expect(overview.chains).toHaveLength(2);
    expect(overview.chains.map((chain) => chain.id)).toEqual([1, 2]);
    expect(overview.chains[0].balanceBaseUnits).toBe(1000000n);
    expect(overview.chains[1].anchorFeeBaseUnits).toBe(1000n);
    expect(overview.totalAnchorSpendBaseUnits).toBe(1000n);
    expect(overview.anchorAvailability).toBe("test-only");
    expect(overview.chains[1].ownerKind).toBe("alias");
    expect(overview.chains[1].accountAddress).toBe(ownerAddress);
  });

  it("preserves unavailable accounting instead of inventing zeroes", () => {
    const overview = normalizeAppChainOverview({
      success: true,
      chains: [{ id: 1, name: "ReadOnly", owner: "operator", anchor_availability: "not-implemented" }],
      totals: {},
    });

    expect(overview.chains[0].balanceBaseUnits).toBeNull();
    expect(overview.chains[0].anchorSpendBaseUnits).toBeNull();
    expect(overview.chains[0].anchorCount).toBeNull();
    expect(overview.totalAnchorCount).toBeNull();
  });

  it("classifies canonical addresses and aliases", () => {
    expect(getAppChainOwnerKind(ownerAddress)).toBe("address");
    expect(getAppChainOwnerKind("Ultra Labs")).toBe("alias");
    expect(getAppChainOwnerKind("   ")).toBe("unknown");
  });

  it("rejects an explicit API failure", () => {
    expect(() => normalizeAppChainOverview({ success: false, message: "node unavailable" })).toThrow("node unavailable");
  });
});
