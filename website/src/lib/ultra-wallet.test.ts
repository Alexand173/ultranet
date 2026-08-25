import { describe, expect, it } from "vitest";
import { buildValidatorProposalRequest, isSignedTransaction, isSignedValidatorProposal } from "./ultra-wallet";

function validTransaction() {
  return {
    sender: "a".repeat(64),
    sender_public_key: Array.from({ length: 2592 }, () => 7),
    recipient: "b".repeat(64),
    amount: 25000000,
    fee: 250000,
    nonce: 0,
    timestamp: 1700000000,
    nullifier: Array.from({ length: 32 }, () => 1),
    gas_limit: 500000,
    gas_price: 1,
    signature: Array.from({ length: 4627 }, () => 9),
    chain_id: 0,
    version: 1,
  };
}

function validValidatorProposal() {
  return {
    sender: "a".repeat(64),
    sender_public_key: Array.from({ length: 2592 }, () => 7),
    proposal_public_key: Array.from({ length: 2592 }, () => 8),
    nonce: 0,
    timestamp: 1700000000,
    nullifier: Array.from({ length: 32 }, () => 1),
    signature: Array.from({ length: 4627 }, () => 9),
    version: 2 as const,
  };
}

describe("ultranet_signValidatorProposal response validation", () => {
  it("accepts and sanitizes the public signed proposal shape", () => {
    const signed = validValidatorProposal();
    expect(isSignedValidatorProposal(signed)).toBe(true);
    expect(buildValidatorProposalRequest(signed, " Genesis-Alpha-01 ")).toEqual({
      ...signed,
      metadata: "Genesis-Alpha-01",
    });
  });

  it("rejects a proposal with a truncated node public key", () => {
    expect(isSignedValidatorProposal({ ...validValidatorProposal(), proposal_public_key: [8] })).toBe(false);
  });
});

describe("ultranet_signTransaction response validation", () => {
  it("accepts the exact public transaction wire shape", () => {
    expect(isSignedTransaction(validTransaction())).toBe(true);
  });

  it.each([
    ["sender", "0x" + "a".repeat(62)],
    ["sender_public_key", [1]],
    ["signature", [1]],
    ["nullifier", [1]],
    ["version", 2],
  ] as const)("rejects malformed %s", (field, value) => {
    expect(isSignedTransaction({ ...validTransaction(), [field]: value })).toBe(false);
  });
});
