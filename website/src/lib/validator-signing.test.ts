import { describe, expect, it } from "vitest";
import {
  buildValidatorProposalDigest,
  createLocalValidatorProvider,
  getValidatorProposalTimestampDiagnostic,
  normalizeProposalPublicKey,
  VALIDATOR_PROPOSAL_FUTURE_TOLERANCE_SECONDS,
  VALIDATOR_PROPOSAL_MAX_AGE_SECONDS,
  validateProposalPublicKey,
} from "./validator-signing";
import { ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL, ULTRA_WALLET_SIGNING_ENVELOPE_VERSION } from "./ultra-wallet";

const publicKey = "ab".repeat(2_592);

function proposalParams(metadata = "Genesis-Alpha-01") {
  return {
    metadata,
    proposalPublicKey: publicKey,
    version: ULTRA_WALLET_SIGNING_ENVELOPE_VERSION,
  } as const;
}

describe("validator proposal public-key validation", () => {
  it("accepts the complete key and normalizes an optional prefix", () => {
    expect(validateProposalPublicKey(`0x${publicKey}`)).toBeNull();
    expect(normalizeProposalPublicKey(`0x${publicKey.toUpperCase()}`)).toBe(publicKey);
  });

  it.each([
    ["an empty value", ""],
    ["a shortened value", "ab"],
    ["a value with non-hex characters", `${"ab".repeat(2_591)}gg`],
  ])("rejects %s", (_label, value) => {
    expect(validateProposalPublicKey(value)).toBeTruthy();
  });
});

describe("validator proposal timestamp diagnostics", () => {
  const clientTimestamp = 1_800_000_000;

  it.each([
    ["fresh", clientTimestamp, "fresh"],
    ["future within tolerance", clientTimestamp + 1, "future_within_tolerance"],
    ["future beyond tolerance", clientTimestamp + VALIDATOR_PROPOSAL_FUTURE_TOLERANCE_SECONDS + 1, "future"],
    ["stale", clientTimestamp - VALIDATOR_PROPOSAL_MAX_AGE_SECONDS - 1, "stale"],
  ] as const)("classifies a %s timestamp without payload data", (_label, proposalTimestamp, classification) => {
    expect(getValidatorProposalTimestampDiagnostic(proposalTimestamp, clientTimestamp)).toEqual({
      event: "validator_proposal_timestamp",
      proposal_timestamp: proposalTimestamp,
      client_timestamp: clientTimestamp,
      age_seconds: clientTimestamp - proposalTimestamp,
      classification,
    });
  });

  it("marks malformed timestamp values invalid and redacted", () => {
    expect(getValidatorProposalTimestampDiagnostic("1700000000", clientTimestamp)).toEqual({
      event: "validator_proposal_timestamp",
      proposal_timestamp: null,
      client_timestamp: clientTimestamp,
      age_seconds: null,
      classification: "invalid",
    });
  });
});

describe("validator proposal signing envelope", () => {
  const baseInput = {
    sender: "11".repeat(32),
    proposalPublicKey: Uint8Array.from({ length: 2_592 }, (_, index) => index % 256),
    metadata: "Genesis-Alpha-01",
    nonce: 4,
    timestamp: 1_700_000_000,
    nullifier: Uint8Array.from({ length: 32 }, (_, index) => index),
  };

  it("is deterministic for the same payload and changes when metadata changes", () => {
    const first = buildValidatorProposalDigest(baseInput);
    const second = buildValidatorProposalDigest({ ...baseInput });
    const changed = buildValidatorProposalDigest({ ...baseInput, metadata: "Genesis-Beta-02" });

    expect(Array.from(first)).toEqual(Array.from(second));
    expect(Array.from(first)).not.toEqual(Array.from(changed));
  });

  it("refuses to sign when the local session is locked", async () => {
    const provider = createLocalValidatorProvider(() => null);

    await expect(provider.request({
      method: ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL,
      params: proposalParams(),
    })).rejects.toThrow("local wallet is locked");
  });
});
