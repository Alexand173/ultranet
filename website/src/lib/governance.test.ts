import { beforeEach, describe, expect, it, vi } from "vitest";
import { authenticatedFetch } from "@/lib/auth";
import {
  createApprovalIntent,
  fetchValidatorReview,
  GovernanceRequestError,
  formatApprovalProgress,
} from "@/lib/governance";

vi.mock("@/lib/auth", () => ({
  authenticatedFetch: vi.fn(),
}));

const mockedAuthenticatedFetch = vi.mocked(authenticatedFetch);
const publicKey = "ab".repeat(2_592);
const proposalHash = "11".repeat(32);
const proposer = "22".repeat(32);

function reviewResponse(overrides: Record<string, unknown> = {}) {
  return {
    success: true,
    proposals: [
      {
        proposal_hash: proposalHash,
        public_key: publicKey,
        metadata: "Genesis-Alpha-01",
        proposer,
        timestamp: 1_785_183_488,
        intent_id: null,
        signed_owner_count: 0,
        current_owner_signed: false,
        threshold: 2,
        total_owners: 3,
      },
    ],
    capabilities: { capabilities: ["validator_review", "sovereign_approve"] },
    ...overrides,
  };
}

describe("validator approval API boundary", () => {
  beforeEach(() => {
    mockedAuthenticatedFetch.mockReset();
    vi.stubGlobal("fetch", vi.fn());
  });

  it("normalizes a protected review response without exposing signer internals", async () => {
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify(reviewResponse()), { status: 200 }));

    await expect(fetchValidatorReview()).resolves.toEqual({
      capabilities: { capabilities: ["validator_review", "sovereign_approve"] },
      proposals: [{
        proposalHash,
        publicKey,
        metadata: "Genesis-Alpha-01",
        proposer,
        submittedAt: 1_785_183_488,
        intentId: null,
        signedOwnerCount: 0,
        currentOwnerSigned: false,
        threshold: 2,
        totalOwners: 3,
      }],
    });
  });

  it("rejects malformed validator public keys from the node", async () => {
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify(reviewResponse({
      proposals: [{ ...reviewResponse().proposals[0], public_key: "ab" }],
    })), { status: 200 }));

    await expect(fetchValidatorReview()).rejects.toEqual(expect.objectContaining({
      name: "GovernanceRequestError",
      code: "CONTRACT_ERROR",
    } satisfies Partial<GovernanceRequestError>));
  });

  it("sends only the selected and human-confirmed proposal hash", async () => {
    mockedAuthenticatedFetch.mockResolvedValue(new Response(JSON.stringify({
      success: true,
      data: {
        intent_id: "33".repeat(24),
        proposal_hash: proposalHash,
        stage: "created",
        signed_owner_count: 0,
        threshold: 2,
        expires_at: 1_785_184_088,
        activated: false,
        message: "Approval intent created",
      },
    }), { status: 200 }));

    await createApprovalIntent(proposalHash, proposalHash);

    expect(mockedAuthenticatedFetch).toHaveBeenCalledWith("/api/governance/approval-intents", {
      method: "POST",
      body: JSON.stringify({ proposal_hash: proposalHash, confirmed_proposal_hash: proposalHash }),
    });
    const requestBody = JSON.parse(mockedAuthenticatedFetch.mock.calls[0][1]?.body as string) as Record<string, unknown>;
    expect(Object.keys(requestBody).sort()).toEqual(["confirmed_proposal_hash", "proposal_hash"]);
  });

  it("formats progress without exceeding the configured threshold", () => {
    expect(formatApprovalProgress(0)).toBe("0/2");
    expect(formatApprovalProgress(1)).toBe("1/2");
    expect(formatApprovalProgress(3)).toBe("2/2");
  });
});
