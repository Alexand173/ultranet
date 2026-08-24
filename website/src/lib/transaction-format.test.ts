import { describe, expect, it } from "vitest";
import {
  checkedAdd,
  checkedSubtract,
  formatAddress,
  formatUltraAmount,
  isUltraAddress,
  parseUltraAmount,
  toSafeNumber,
} from "./transaction-format";

describe("transaction amount formatting", () => {
  it("parses six-decimal ULTRA amounts as base units", () => {
    expect(parseUltraAmount("25.000001")).toEqual({ value: 25000001n, error: null });
    expect(parseUltraAmount(" 1.2 ")).toEqual({ value: 1200000n, error: null });
  });

  it.each(["", "0", "-1", "1e3", "1.2345678", "not-a-number"])("rejects unsafe amount %s", (value) => {
    expect(parseUltraAmount(value).value).toBeNull();
  });

  it("formats base units without floating-point rounding", () => {
    expect(formatUltraAmount(25000000n)).toBe("25.000000");
    expect(formatUltraAmount(123456789012n)).toBe("123,456.789012");
    expect(formatUltraAmount(-1n)).toBe("-0.000001");
  });

  it("checks totals and safe JSON conversion", () => {
    expect(checkedAdd(25000000n, 250000n, "Total")).toBe(25250000n);
    expect(checkedSubtract(10n, 4n)).toBe(6n);
    expect(checkedSubtract(4n, 10n)).toBeNull();
    expect(toSafeNumber(25250000n, "Amount")).toBe(25250000);
  });
});

describe("UltraNet addresses", () => {
  it("accepts only lowercase 64-character hexadecimal addresses", () => {
    const address = "a".repeat(64);
    expect(isUltraAddress(address)).toBe(true);
    expect(isUltraAddress(address.toUpperCase())).toBe(false);
    expect(isUltraAddress(`${address}0`)).toBe(false);
    expect(formatAddress(address)).toBe("aaaaaaaaaa…aaaaaaaa");
  });
});
