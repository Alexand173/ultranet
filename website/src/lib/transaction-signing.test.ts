import { describe, expect, it } from "vitest";
import { buildLegacyTransactionDigest } from "./transaction-signing";

function toHex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("UltraNet version-1 transaction digest", () => {
  it("matches the Rust field order and little-endian encoding", () => {
    const digest = buildLegacyTransactionDigest({
      sender: "11".repeat(32),
      recipient: "22".repeat(32),
      amount: 25000000,
      fee: 250000,
      timestamp: 1700000000,
      nullifier: Uint8Array.from({ length: 32 }, (_, index) => index),
      nonce: 0,
      gasLimit: 500000,
      gasPrice: 1,
    });

    expect(toHex(digest)).toBe("f968acd8ef5f17f72eed6d71d1c4ba9a03de4bbb5c28f3da2718ef6d18079c72");
  });

  it("rejects nullifiers that are not exactly 32 bytes", () => {
    expect(() => buildLegacyTransactionDigest({
      sender: "11".repeat(32),
      recipient: "22".repeat(32),
      amount: 1,
      fee: 1,
      timestamp: 1700000000,
      nullifier: new Uint8Array(31),
      nonce: 0,
      gasLimit: 500000,
      gasPrice: 1,
    })).toThrow("32 bytes");
  });
});
