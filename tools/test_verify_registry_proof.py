#!/usr/bin/env python3
"""Tests for verify_registry_proof.py. Run: python3 -I -m unittest tools/test_verify_registry_proof.py -v

The expected values are the ones pinned in src/games/registry.rs and ownership.rs and in the Node SDK's
tests, so this file is the third implementation checked against the same vectors."""
import copy
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location("v", pathlib.Path(__file__).with_name("verify_registry_proof.py"))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)

A = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"
B = "11" * 32
TETRIS = [{"player": B, "result_id": "match-0002", "score": 9, "data_hash": "02" * 32},
          {"player": A, "result_id": "match-0001", "score": 5, "data_hash": "01" * 32}]
SNAKE = [{"player": A, "result_id": "round-7", "score": 1000}]
PINNED = {
    "tetris": "a9548ebbb42d3fed507a94995fae1b5066f84b169bc83221b6811bb60ee6b744", "snake": "d64fdd3860a91abc4ced3dd26c56788ab94e8b996887d87eeac1fc1c4bbb1859", "merkle": "8aee448bb31e3c53e33e27239a3e313478d8d29fff7dcf179b0b8b5907d2ccc8",
    "day": "c003c41c297e990fb0264d4cbdb9c8958622d3b3fcb6cbee50eade9246ad2823", "zeta_sibling": "9fafc8c72f9953e313109dea0990ed2595c4d47812cc1018cbc58f662dd6f96c",
}


def fixture():
    tetris, snake, zeta = (v.game_results_root("tetris", TETRIS), v.game_results_root("snake", SNAKE),
                           v.game_results_root("zeta", []))
    leaves = [v.leaf_hash("snake", snake), v.leaf_hash("tetris", tetris), v.leaf_hash("zeta", zeta)]
    ab = v.sha3(b"UltraRecord/node/v1", leaves[0], leaves[1])
    merkle = v.sha3(b"UltraRecord/node/v1", ab, leaves[2])
    day = v.sha3(b"UltraRecord/day-root/v1", b"2026-11-16", merkle)
    return tetris, snake, ab, merkle, day, leaves


class VerifyRegistryProof(unittest.TestCase):
    def proof(self, game="zeta"):
        tetris, snake, ab, merkle, day, leaves = fixture()
        base = {"date": "2026-11-16", "merkle_root": merkle.hex(), "day_root": day.hex()}
        if game == "zeta":
            return {**base, "game_id": "zeta", "game_root": v.game_results_root("zeta", []).hex(),
                    "leaf": leaves[2].hex(), "steps": [{"sibling": ab.hex(), "side": "left"}]}
        return {**base, "game_id": "tetris", "game_root": tetris.hex(), "leaf": leaves[1].hex(),
                "steps": [{"sibling": leaves[0].hex(), "side": "left"}, {"sibling": leaves[2].hex(), "side": "right"}]}

    def test_roots_match_the_vectors_pinned_in_rust_and_the_node_sdk(self):
        tetris, snake, ab, merkle, day, _ = fixture()
        self.assertEqual(tetris.hex(), PINNED["tetris"])
        self.assertEqual(snake.hex(), PINNED["snake"])
        self.assertEqual(ab.hex(), PINNED["zeta_sibling"])
        self.assertEqual(merkle.hex(), PINNED["merkle"])
        self.assertEqual(day.hex(), PINNED["day"])

    def test_no_old_tag_is_left_in_the_verifier(self):
        self.assertNotIn("UltraGame", pathlib.Path(v.__file__).read_text())

    def test_a_genuine_proof_holds_with_and_without_the_developers_results(self):
        self.assertEqual(v.verify(self.proof()), [])
        self.assertEqual(v.verify(self.proof("tetris"), TETRIS), [])

    def test_bent_proofs_and_altered_results_are_caught(self):
        bent = copy.deepcopy(self.proof()); bent["steps"][0]["sibling"] = "00" * 32
        self.assertEqual(v.verify(bent), ["the proof does not climb to the merkle root"])
        flipped = copy.deepcopy(self.proof()); flipped["steps"][0]["side"] = "right"
        self.assertEqual(len(v.verify(flipped)), 1)
        wrong_day = copy.deepcopy(self.proof()); wrong_day["day_root"] = "00" * 32
        self.assertEqual(v.verify(wrong_day), ["the day root is not the hash of the date and the merkle root"])
        altered = [dict(TETRIS[0], score=10), TETRIS[1]]
        self.assertEqual(v.verify(self.proof("tetris"), altered),
                         ["the results you supplied do not produce the game root in the proof"])

    def test_a_proof_made_with_the_old_tags_no_longer_verifies(self):
        # Built the way the previous format built it. It must fail, or the rename did not take effect.
        old_leaf = v.sha3(b"UltraGame/leaf/v1", len(b"zeta").to_bytes(4, "little"), b"zeta",
                          v.game_results_root("zeta", []))
        proof = self.proof(); proof["leaf"] = old_leaf.hex()
        self.assertIn("the leaf is not the hash of this game id and game root", v.verify(proof))


if __name__ == "__main__":
    unittest.main()
