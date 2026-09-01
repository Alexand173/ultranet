import { API_BASE_URL } from "@/lib/links";
import type { AuthLoginPayload } from "@/lib/auth-payload";
import {
  buildAuthenticationLoginRequest,
  getUltraWalletProvider,
  isSignedAuthenticationChallenge,
  type AuthChallenge,
  type AuthSessionPayload,
  type SignedAuthenticationChallenge,
} from "@/lib/ultra-wallet";

interface ApiEnvelope<T> {
  success?: boolean;
  message?: string;
  data?: T;
}

export interface AuthSession {
  nodeIdentifier: string;
  expiresAt: number;
}

function apiError(payload: ApiEnvelope<unknown>, fallback: string): Error {
  return new Error(typeof payload.message === "string" && payload.message.trim() ? payload.message : fallback);
}

async function readJson<T>(response: Response): Promise<ApiEnvelope<T>> {
  const payload = (await response.json().catch(() => null)) as ApiEnvelope<T> | null;
  if (!payload) throw new Error(`UltraNet API returned ${response.status}`);
  return payload;
}

function buildRequestHeaders(init: RequestInit, csrfToken?: string): Headers {
  const headers = new Headers(init.headers);
  if (!headers.has("Accept")) headers.set("Accept", "application/json");
  if (typeof init.body === "string" && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (csrfToken) headers.set("X-UltraNet-CSRF", csrfToken);
  return headers;
}

async function request<T>(path: string, init: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...init,
    credentials: "include",
    cache: "no-store",
    headers: buildRequestHeaders(init),
  });
  const payload = await readJson<T>(response);
  if (!response.ok || payload.success === false || payload.data === undefined) {
    throw apiError(payload, `UltraNet API returned ${response.status}`);
  }
  return payload.data;
}

export async function requestAuthChallenge(nodeIdentifier: string): Promise<AuthChallenge> {
  return request<AuthChallenge>("/api/auth/challenge", {
    method: "POST",
    body: JSON.stringify({ node_identifier: nodeIdentifier.trim() }),
  });
}

export async function signAuthChallenge(challenge: AuthChallenge): Promise<SignedAuthenticationChallenge> {
  const wallet = getUltraWalletProvider();
  if (!wallet) {
    throw new Error("UltraWallet is required. Install or unlock it before initializing a session.");
  }

  const signed = await wallet.request({
    method: "ultranet_signAuthenticationChallenge",
    params: {
      challengeId: challenge.challengeId,
      challenge: challenge.challenge,
      nodeIdentifier: challenge.nodeIdentifier,
      expiresAt: challenge.expiresAt,
      version: challenge.version,
    },
  });
  if (!isSignedAuthenticationChallenge(signed)) {
    throw new Error("UltraWallet returned an incomplete authentication signature.");
  }
  return signed;
}

async function submitAuthLoginPayload(payload: AuthLoginPayload): Promise<AuthSession> {
  const data = await request<AuthSessionPayload>("/api/auth/login", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  return {
    nodeIdentifier: data.node_identifier,
    expiresAt: data.expires_at,
  };
}

export async function loginWithSignedChallenge(
  challenge: AuthChallenge,
  signed: SignedAuthenticationChallenge,
): Promise<AuthSession> {
  return submitAuthLoginPayload(buildAuthenticationLoginRequest(challenge, signed));
}

export async function loginWithAuthLoginPayload(payload: AuthLoginPayload): Promise<AuthSession> {
  return submitAuthLoginPayload(payload);
}

export async function fetchAuthSession(): Promise<AuthSession | null> {
  const response = await fetch(`${API_BASE_URL}/api/auth/session`, {
    credentials: "include",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  if (response.status === 401) return null;
  const data = await readJson<{ node_identifier: string; expires_at: number }>(response);
  if (!response.ok || data.success === false || !data.data) {
    throw apiError(data, `UltraNet API returned ${response.status}`);
  }
  return {
    nodeIdentifier: data.data.node_identifier,
    expiresAt: data.data.expires_at,
  };
}

function readCookie(name: string): string | null {
  if (typeof document === "undefined") return null;
  const prefix = `${name}=`;
  const value = document.cookie.split(";").map((part) => part.trim()).find((part) => part.startsWith(prefix));
  return value ? decodeURIComponent(value.slice(prefix.length)) : null;
}

export async function authenticatedFetch(path: string, init: RequestInit = {}): Promise<Response> {
  const method = (init.method ?? "GET").toUpperCase();
  const csrfToken = !["GET", "HEAD", "OPTIONS"].includes(method) ? readCookie("ultranet_csrf") : null;
  return fetch(`${API_BASE_URL}${path}`, {
    ...init,
    credentials: "include",
    cache: "no-store",
    headers: buildRequestHeaders(init, csrfToken ?? undefined),
  });
}

export async function logout(): Promise<void> {
  const response = await authenticatedFetch("/api/auth/logout", { method: "POST" });
  if (!response.ok) {
    const payload = await readJson<unknown>(response);
    throw apiError(payload, `UltraNet API returned ${response.status}`);
  }
}
