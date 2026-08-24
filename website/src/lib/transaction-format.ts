import { ULTRA_DECIMALS } from "@/lib/wallet-crypto";

const MAX_SAFE_INTEGER_BIGINT = BigInt(Number.MAX_SAFE_INTEGER);

export type AmountParseResult =
  | { value: bigint; error: null }
  | { value: null; error: string };

export interface ParsedAmount {
  readonly value: bigint;
  readonly display: string;
}

export function parseUltraAmount(
  input: string,
  decimals: number = ULTRA_DECIMALS,
): AmountParseResult {
  const normalized = input.trim();
  if (!normalized) return { value: null, error: "Enter an amount greater than zero." };
  if (!/^\d+(?:\.\d+)?$/.test(normalized)) {
    return { value: null, error: "Enter a valid ULTRA amount with up to 6 decimal places." };
  }

  const [whole, fraction = ""] = normalized.split(".");
  if (fraction.length > decimals) {
    return { value: null, error: `Use no more than ${decimals} decimal places.` };
  }

  try {
    const base = 10n ** BigInt(decimals);
    const fractionalUnits = BigInt(fraction.padEnd(decimals, "0") || "0");
    const value = BigInt(whole) * base + fractionalUnits;
    if (value <= 0n) return { value: null, error: "Enter an amount greater than zero." };
    if (value > MAX_SAFE_INTEGER_BIGINT) return { value: null, error: "This amount is too large for a browser transaction." };
    return { value, error: null };
  } catch {
    return { value: null, error: "Enter a valid ULTRA amount." };
  }
}

export function formatUltraAmount(value: bigint | number | null, decimals: number = ULTRA_DECIMALS): string {
  if (value === null) return "—";
  const baseUnits = typeof value === "number" ? BigInt(value) : value;
  const negative = baseUnits < 0n;
  const unsigned = negative ? -baseUnits : baseUnits;
  const base = 10n ** BigInt(decimals);
  const whole = unsigned / base;
  const fraction = (unsigned % base).toString().padStart(decimals, "0");
  const groupedWhole = whole.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${negative ? "-" : ""}${groupedWhole}.${fraction}`;
}

export function toSafeNumber(value: bigint, label: string): number {
  if (value < 0n || value > MAX_SAFE_INTEGER_BIGINT) {
    throw new Error(`${label} exceeds the browser transaction limit.`);
  }
  return Number(value);
}

export function isUltraAddress(value: string): boolean {
  return /^[0-9a-f]{64}$/.test(value.trim());
}

export function formatAddress(value: string, start = 10, end = 8): string {
  const normalized = value.trim();
  if (normalized.length <= start + end + 1) return normalized;
  return `${normalized.slice(0, start)}…${normalized.slice(-end)}`;
}

export function formatTimestamp(timestamp: number): string {
  if (!Number.isSafeInteger(timestamp) || timestamp <= 0) return "Unknown time";
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp * 1000));
}

export function checkedAdd(left: bigint, right: bigint, label: string): bigint {
  const result = left + right;
  if (result > MAX_SAFE_INTEGER_BIGINT) throw new Error(`${label} exceeds the browser transaction limit.`);
  return result;
}

export function checkedSubtract(left: bigint, right: bigint): bigint | null {
  return left >= right ? left - right : null;
}
