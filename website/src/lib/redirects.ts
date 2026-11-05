/**
 * Read the `?next=` destination from the browser's address bar.
 *
 * Reading this during render makes the server and client disagree: the server
 * has no location, the client does. Call it from an effect or an event handler
 * instead, where one answer is the only answer.
 */
export function readReturnPath(): string | null {
  if (typeof window === "undefined") return null;
  return getSafeReturnPath(new URLSearchParams(window.location.search).get("next"));
}

export function getSafeReturnPath(value: string | null | undefined): string | null {
  if (
    typeof value !== "string" ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("\\") ||
    /[\u0000-\u001f]/.test(value)
  ) {
    return null;
  }

  try {
    const url = new URL(value, "https://ultranet.invalid");
    return `${url.pathname}${url.search}${url.hash}`;
  } catch {
    return null;
  }
}
