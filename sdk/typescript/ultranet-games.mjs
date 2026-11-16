// UltraNet Games SDK for game servers (Node 18+, no dependencies).
//
//   import { GamesClient } from "./ultranet-games.mjs";
//   const games = new GamesClient({ gameId: "my-game", apiKey: process.env.ULTRANET_GAME_KEY });
//   await games.submitResult({ player: "<64 hex>", resultId: "match-0001", score: 1200 });
//   const check = await games.verifyDay("2026-11-16", myResultsThatDay);   // { ok, problems, anchored }
//
// Keep the API key on your server. The platform records the results you report and makes them
// tamper-evident; it does not check that they are true and it moves no money.
// Layouts: src/games/registry.rs (roots and proofs) and src/games/ownership.rs (owner proofs).

import { createHash } from "node:crypto";

export const DEFAULT_API = "https://api.ultranetwork.cc";

const sha3 = (...parts) => {
  const h = createHash("sha3-256");
  for (const p of parts) h.update(p);
  return h.digest();
};
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const hex = (s, bytes, label) => {
  if (typeof s !== "string" || !/^[0-9a-f]+$/.test(s) || s.length !== bytes * 2) {
    throw new Error(`${label} must be ${bytes * 2} lowercase hex characters`);
  }
  return Buffer.from(s, "hex");
};
const NODE = Buffer.from("UltraGame/node/v1");

/** The root of one game's results for one day. `results`: [{player, resultId, score?, dataHash?}] */
export function gameResultsRoot(gameId, results) {
  const records = results.map((r) => ({
    player: hex(r.player, 32, "player"),
    idHash: sha3(Buffer.from("UltraGame/result-id/v1"), Buffer.from(r.resultId)),
    score: r.score ?? 0,
    data: r.dataHash ? hex(r.dataHash, 32, "dataHash") : Buffer.alloc(32),
  }));
  records.sort((a, b) => Buffer.compare(a.player, b.player) || Buffer.compare(a.idHash, b.idHash));
  const id = Buffer.from(gameId);
  return sha3(
    Buffer.from("UltraGame/results/v1"), u32(id.length), id, u32(records.length),
    ...records.flatMap((r) => [r.player, r.idHash, u64(r.score), r.data]),
  );
}

export const leafHash = (gameId, gameRoot) => {
  const id = Buffer.from(gameId);
  return sha3(Buffer.from("UltraGame/leaf/v1"), u32(id.length), id, gameRoot);
};

/** Check an inclusion proof as returned by GET /api/games/g/<id>/proof/<date>. Returns problems ([] = holds). */
export function verifyProof(proof, results) {
  const problems = [];
  const gameRoot = hex(proof.game_root, 32, "game_root");
  const leaf = leafHash(proof.game_id, gameRoot);
  if (leaf.toString("hex") !== proof.leaf) problems.push("the leaf is not the hash of this game id and game root");
  let current = leaf;
  for (const step of proof.steps) {
    const sibling = hex(step.sibling, 32, "sibling");
    current = step.side === "left" ? sha3(NODE, sibling, current) : sha3(NODE, current, sibling);
  }
  if (current.toString("hex") !== proof.merkle_root) problems.push("the proof does not climb to the merkle root");
  const dayRoot = sha3(Buffer.from("UltraGame/day-root/v1"), Buffer.from(proof.date), hex(proof.merkle_root, 32, "merkle_root"));
  if (dayRoot.toString("hex") !== proof.day_root) problems.push("the day root is not the hash of the date and the merkle root");
  if (results && !gameResultsRoot(proof.game_id, results).equals(gameRoot)) {
    problems.push("the results you supplied do not produce the game root in the proof");
  }
  return problems;
}

/** The digest an owner signs (Dilithium5) to register a game or rotate its key. */
export function ownerProofDigest(action, gameId, name, timestamp) {
  const part = (s) => [u32(Buffer.byteLength(s)), Buffer.from(s)];
  return sha3(Buffer.from("UltraGame/owner-proof/v1"), ...part(action), ...part(gameId), ...part(name), u64(timestamp));
}

export class GamesApiError extends Error {
  constructor(status, code, message) { super(`${code} (${status}): ${message}`); this.status = status; this.code = code; }
}

export class GamesClient {
  constructor({ gameId, apiKey, baseUrl = DEFAULT_API, fetchImpl = globalThis.fetch }) {
    this.gameId = gameId; this.apiKey = apiKey; this.baseUrl = baseUrl.replace(/\/$/, ""); this.fetch = fetchImpl;
  }

  async #call(method, path, body, auth) {
    const response = await this.fetch(`${this.baseUrl}/api/games/g/${this.gameId}${path}`, {
      method,
      headers: { "Content-Type": "application/json", ...(auth ? { Authorization: `Bearer ${this.apiKey}` } : {}) },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const reply = await response.json().catch(() => ({}));
    if (!response.ok) throw new GamesApiError(response.status, reply.code ?? "error", reply.message ?? "no message");
    return reply;
  }

  /** Report one result. Safe to retry: the same resultId with the same contents is a no-op (`duplicate: true`). */
  submitResult({ player, resultId, score = 0, dataHash }) {
    return this.#call("POST", "/results", {
      player, result_id: resultId, score, ...(dataHash ? { data_hash: dataHash } : {}),
    }, true);
  }

  proof(date) { return this.#call("GET", `/proof/${date}`); }

  /** Fetch the day's proof and check it, and (if you pass them) that your own records produce the root. */
  async verifyDay(date, results) {
    const proof = await this.proof(date);
    const problems = verifyProof(proof, results);
    return { ok: problems.length === 0, problems, anchored: Boolean(proof.anchor_tx), state: proof.state, anchorTx: proof.anchor_tx ?? null };
  }
}
