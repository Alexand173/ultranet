import { describe, expect, it } from "vitest";
import { deriveDilithiumSeedFromPhrase, isRecoveryPhraseValid, normalizeRecoveryPhrase } from "./wallet-crypto";

function toHex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("local wallet recovery derivation", () => {
  const phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

  it("normalizes and validates the BIP39 phrase", () => {
    expect(normalizeRecoveryPhrase("  ABANDON   abandon about ")).toBe("abandon abandon about");
    expect(isRecoveryPhraseValid(phrase)).toBe(true);
    expect(isRecoveryPhraseValid("abandon abandon abandon")).toBe(false);
  });

  it("uses the versioned UltraNet domain separator", () => {
    expect(toHex(deriveDilithiumSeedFromPhrase(phrase))).toBe("86b21c2b9e5d6e4f6d71f324da1db33565ece8c86448a1eb1592a5271aae2847");
  });
});
