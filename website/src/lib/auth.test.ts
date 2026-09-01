import { afterEach, describe, expect, it, vi } from "vitest";
import { authenticatedFetch } from "@/lib/auth";

describe("authenticatedFetch", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("sets JSON content type for string request bodies", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await authenticatedFetch("/api/governance/approval-intents", {
      method: "POST",
      body: JSON.stringify({ proposal_hash: "11".repeat(32) }),
    });

    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    const headers = new Headers(request.headers);
    expect(headers.get("accept")).toBe("application/json");
    expect(headers.get("content-type")).toBe("application/json");
  });

  it("preserves an explicit content type for non-JSON requests", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);

    await authenticatedFetch("/api/upload", {
      method: "POST",
      body: "plain text",
      headers: { "Content-Type": "text/plain" },
    });

    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(new Headers(request.headers).get("content-type")).toBe("text/plain");
  });

  it("forwards the readable CSRF cookie on unsafe requests", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("document", { cookie: "ultranet_csrf=csrf-token-123" });

    await authenticatedFetch("/api/governance/approval-intents", { method: "POST", body: "{}" });

    const request = fetchMock.mock.calls[0]?.[1] as RequestInit;
    expect(new Headers(request.headers).get("x-ultranet-csrf")).toBe("csrf-token-123");
  });
});
