import { beforeEach, describe, expect, it, vi } from "vitest";
import { authenticatedFetch } from "@/lib/auth";
import { normalizeNetworkStats } from "@/lib/api";
import { mineBlock, OperatorRequestError } from "@/lib/operator";

vi.mock("@/lib/auth", () => ({
  authenticatedFetch: vi.fn(),
}));

const mockedAuthenticatedFetch = vi.mocked(authenticatedFetch);

describe("operator API helpers", () => {
  beforeEach(() => {
    mockedAuthenticatedFetch.mockReset();
  });

  it("normalizes real node fields and preserves unavailable measurements", () => {
    const stats = normalizeNetworkStats({
      success: true,
      data: {
        total_blocks: "12",
        validator_count: "5",
        total_weight: "500",
        mempool_size: "2",
        total_transactions: "18",
      },
    });

    expect(stats).toMatchObject({
      totalBlocks: 12,
      validatorCount: 5,
      totalWeight: 500,
      mempoolSize: 2,
      totalTransactions: 18,
      tps: null,
      provingTimeMs: null,
      activeAppchains: null,
      status: "live",
    });
  });

  it("submits mine through the credentialed session helper without a bearer token", async () => {
    mockedAuthenticatedFetch.mockResolvedValue(
      new Response(JSON.stringify({ success: true, message: "Block 8 successfully added!", data: { index: 8 } }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(mineBlock()).resolves.toEqual({
      index: 8,
      message: "Block 8 successfully added!",
      block: { index: 8 },
    });
    expect(mockedAuthenticatedFetch).toHaveBeenCalledWith("/api/mine", { method: "POST" });
  });

  it("preserves node rejection status and message", async () => {
    mockedAuthenticatedFetch.mockResolvedValue(
      new Response(JSON.stringify({ success: false, message: "Nema validnih transakcija za rudarenje", data: null }), {
        status: 400,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(mineBlock()).rejects.toEqual(
      expect.objectContaining({
        name: "OperatorRequestError",
        status: 400,
        message: "Nema validnih transakcija za rudarenje",
      } satisfies Partial<OperatorRequestError>),
    );
  });

  it("maps network failure to a retryable operator error", async () => {
    mockedAuthenticatedFetch.mockRejectedValue(new Error("offline"));

    await expect(mineBlock()).rejects.toEqual(
      expect.objectContaining({
        name: "OperatorRequestError",
        status: 0,
        message: "Unable to reach the UltraNet node.",
      }),
    );
  });
});
