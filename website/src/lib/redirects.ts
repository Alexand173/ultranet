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
