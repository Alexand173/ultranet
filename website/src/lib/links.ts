const DEFAULT_API_BASE_URL = "https://api.ultranetwork.cc";

export const EXPLORER_URL =
  process.env.NEXT_PUBLIC_EXPLORER_URL ?? `${DEFAULT_API_BASE_URL}/dashboard`;

export const API_BASE_URL =
  process.env.NEXT_PUBLIC_API_BASE_URL ?? DEFAULT_API_BASE_URL;
