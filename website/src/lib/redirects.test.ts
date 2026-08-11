import { describe, expect, it } from "vitest";
import { getSafeReturnPath } from "@/lib/redirects";

describe("getSafeReturnPath", () => {
  it("keeps same-origin relative paths with query and hash", () => {
    expect(getSafeReturnPath("/operator?tab=telemetry#stats")).toBe("/operator?tab=telemetry#stats");
  });

  it("rejects external and protocol-relative destinations", () => {
    expect(getSafeReturnPath("https://example.com/operator")).toBeNull();
    expect(getSafeReturnPath("//example.com/operator")).toBeNull();
    expect(getSafeReturnPath("javascript:alert(1)")).toBeNull();
  });

  it("rejects backslash and control-character tricks", () => {
    expect(getSafeReturnPath("/\\\\example.com")).toBeNull();
    expect(getSafeReturnPath("/operator\u0000")).toBeNull();
  });

  it("rejects missing values", () => {
    expect(getSafeReturnPath(null)).toBeNull();
    expect(getSafeReturnPath(undefined)).toBeNull();
    expect(getSafeReturnPath("operator")).toBeNull();
  });
});
