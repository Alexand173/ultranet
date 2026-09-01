import { describe, expect, it } from "vitest";
import {
  deriveDilithiumSeedFromPhrase,
  isRecoveryPhraseValid,
  normalizeRecoveryPhrase,
  normalizeWalletAddress,
  RECOVERY_PHRASE_WORD_COUNT,
  recoveryPhraseWordCount,
} from "./wallet-crypto";

function toHex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("local wallet recovery derivation", () => {
  const phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
  const fifteenWordPhrase = "style hazard vendor belt client ticket stone country boil borrow jungle now shaft borrow initial";

  it("normalizes and validates the exact 12-word BIP39 phrase", () => {
    expect(normalizeRecoveryPhrase("  ABANDON   abandon about ")).toBe("abandon abandon about");
    expect(recoveryPhraseWordCount(` \n${phrase.toUpperCase()}\t`)).toBe(RECOVERY_PHRASE_WORD_COUNT);
    expect(isRecoveryPhraseValid(phrase)).toBe(true);
    expect(isRecoveryPhraseValid("abandon abandon abandon")).toBe(false);
    expect(isRecoveryPhraseValid("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon")).toBe(false);
    expect(recoveryPhraseWordCount(fifteenWordPhrase)).toBe(15);
    expect(isRecoveryPhraseValid(fifteenWordPhrase)).toBe(false);
    expect(() => deriveDilithiumSeedFromPhrase(fifteenWordPhrase)).toThrow(
      "Enter a valid 12-word recovery phrase.",
    );
  });

  it("normalizes wallet addresses for identity comparisons", () => {
    expect(normalizeWalletAddress("  0XAbCdEf  ")).toBe("abcdef");
  });

  it("uses the versioned UltraNet domain separator", () => {
    expect(toHex(deriveDilithiumSeedFromPhrase(phrase))).toBe("86b21c2b9e5d6e4f6d71f324da1db33565ece8c86448a1eb1592a5271aae2847");
  });
});
