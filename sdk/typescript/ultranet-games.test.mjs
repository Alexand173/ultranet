import test from "node:test";
import assert from "node:assert/strict";
import { gameResultsRoot, leafHash, verifyProof, ownerProofDigest, GamesClient } from "./ultranet-games.mjs";
import { createHash } from "node:crypto";

// The players of the fixture whose roots are pinned in src/games/registry.rs.
const A = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c";
const B = "11".repeat(32);
const tetris = [
  { player: B, resultId: "match-0002", score: 9, dataHash: "02".repeat(32) },
  { player: A, resultId: "match-0001", score: 5, dataHash: "01".repeat(32) },
];
const sha3 = (...p) => { const h = createHash("sha3-256"); p.forEach((x) => h.update(x)); return h.digest(); };

test("roots match the vectors pinned in the Rust code", () => {
  assert.equal(gameResultsRoot("tetris", tetris).toString("hex"), "a9548ebbb42d3fed507a94995fae1b5066f84b169bc83221b6811bb60ee6b744");
  assert.equal(gameResultsRoot("snake", [{ player: A, resultId: "round-7", score: 1000 }]).toString("hex"), "d64fdd3860a91abc4ced3dd26c56788ab94e8b996887d87eeac1fc1c4bbb1859");
});

test("order of results does not matter, contents do", () => {
  assert.deepEqual(gameResultsRoot("tetris", tetris), gameResultsRoot("tetris", [...tetris].reverse()));
  assert.notDeepEqual(gameResultsRoot("tetris", tetris), gameResultsRoot("tetris", [{ ...tetris[0], score: 10 }, tetris[1]]));
});

test("the owner-proof digest matches the Rust and Python pin", () => {
  assert.equal(ownerProofDigest("register", "tetris", "Tetris", 1_800_000_000).toString("hex"),
    "a91aa99df10c32e75e9ad18b8586cd019284721d087ce14c5c02662d1fed0650");
});

function proofFor() {
  const root = gameResultsRoot("tetris", tetris);
  const snake = gameResultsRoot("snake", [{ player: A, resultId: "round-7", score: 1000 }]);
  const zeta = gameResultsRoot("zeta", []);
  const leaves = [leafHash("snake", snake), leafHash("tetris", root), leafHash("zeta", zeta)];
  const NODE = Buffer.from("UltraRecord/node/v1");
  const ab = sha3(NODE, leaves[0], leaves[1]);
  const merkle = sha3(NODE, ab, leaves[2]);
  assert.equal(merkle.toString("hex"), "8aee448bb31e3c53e33e27239a3e313478d8d29fff7dcf179b0b8b5907d2ccc8");
  const day = sha3(Buffer.from("UltraRecord/day-root/v1"), Buffer.from("2026-11-16"), merkle);
  return {
    game_id: "tetris", date: "2026-11-16", game_root: root.toString("hex"), leaf: leaves[1].toString("hex"),
    steps: [{ sibling: leaves[0].toString("hex"), side: "left" }, { sibling: leaves[2].toString("hex"), side: "right" }],
    merkle_root: merkle.toString("hex"), day_root: day.toString("hex"), state: "confirmed", anchor_tx: "ab".repeat(32),
  };
}

test("a genuine proof holds, with and without the developer's own results", () => {
  assert.deepEqual(verifyProof(proofFor()), []);
  assert.deepEqual(verifyProof(proofFor(), tetris), []);
});

test("bent proofs and altered results are caught", () => {
  const bent = proofFor(); bent.steps[0].sibling = "00".repeat(32);
  assert.deepEqual(verifyProof(bent), ["the proof does not climb to the merkle root"]);
  const flipped = proofFor(); flipped.steps[0].side = "right";
  assert.equal(verifyProof(flipped).length, 1);
  const wrongDay = proofFor(); wrongDay.day_root = "00".repeat(32);
  assert.deepEqual(verifyProof(wrongDay), ["the day root is not the hash of the date and the merkle root"]);
  assert.deepEqual(verifyProof(proofFor(), [{ ...tetris[0], score: 10 }, tetris[1]]),
    ["the results you supplied do not produce the game root in the proof"]);
});

test("the client sends the key as a Bearer token, retries nothing, and surfaces API errors", async () => {
  const seen = [];
  const fetchImpl = async (url, init) => {
    seen.push({ url, init });
    if (url.endsWith("/results")) return { ok: false, status: 401, json: async () => ({ code: "invalid_key", message: "the game id or key is not valid" }) };
    return { ok: true, status: 200, json: async () => proofFor() };
  };
  const client = new GamesClient({ gameId: "tetris", apiKey: "ugk_x", fetchImpl });
  await assert.rejects(client.submitResult({ player: A, resultId: "match-0001" }), (e) => e.status === 401 && e.code === "invalid_key");
  assert.equal(seen[0].init.headers.Authorization, "Bearer ugk_x");
  assert.deepEqual(JSON.parse(seen[0].init.body), { player: A, result_id: "match-0001", score: 0 });
  const day = await client.verifyDay("2026-11-16", tetris);
  assert.deepEqual([day.ok, day.anchored, day.state], [true, true, "confirmed"]);
  assert.equal(seen[1].init.headers.Authorization, undefined, "reads carry no key");
});
