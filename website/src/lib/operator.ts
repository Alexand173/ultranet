import { authenticatedFetch } from "@/lib/auth";

export interface MineBlockResult {
  index: number | null;
  message: string;
  block: Record<string, unknown> | null;
}

export class OperatorRequestError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "OperatorRequestError";
  }
}

interface ApiEnvelope<T> {
  success?: boolean;
  message?: string;
  data?: T;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readBlockIndex(block: Record<string, unknown> | null): number | null {
  const value = block?.index;
  if (typeof value === "number" && Number.isSafeInteger(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    return Number.isSafeInteger(parsed) ? parsed : null;
  }
  return null;
}

export async function mineBlock(): Promise<MineBlockResult> {
  let response: Response;
  try {
    response = await authenticatedFetch("/api/mine", { method: "POST" });
  } catch {
    throw new OperatorRequestError("Unable to reach the UltraNet node.", 0);
  }

  const payload = (await response.json().catch(() => null)) as ApiEnvelope<unknown> | null;
  const message =
    typeof payload?.message === "string" && payload.message.trim().length > 0
      ? payload.message
      : `UltraNet API returned ${response.status}`;

  if (!response.ok || payload?.success === false) {
    throw new OperatorRequestError(message, response.status);
  }

  const block = isRecord(payload?.data) ? payload.data : null;
  return {
    index: readBlockIndex(block),
    message,
    block,
  };
}
