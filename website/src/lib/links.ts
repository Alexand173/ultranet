const DEFAULT_API_BASE_URL = "https://api.ultranetwork.cc";

export const EXPLORER_URL =
  process.env.NEXT_PUBLIC_EXPLORER_URL ?? `${DEFAULT_API_BASE_URL}/dashboard`;

export const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_BASE_URL ?? DEFAULT_API_BASE_URL;

export const SEND_ULTRA_PATH = "/send-ultra" as const;
export const VALIDATOR_PROPOSAL_PATH = "/#swarm" as const;

export function getSendUltraHref(returnTo?: string): string {
  if (!returnTo) return SEND_ULTRA_PATH;
  const search = new URLSearchParams({ returnTo });
  return `${SEND_ULTRA_PATH}?${search.toString()}`;
}
