#!/usr/bin/env python3
"""Refuse the retired sovereign transfer workflow.

This file intentionally does not import a PQC implementation, read key files,
create transfer.json, or call the production API. The former version embedded a
stale nonce/amount and submitted a generated artifact immediately, which was
unsafe after the six-decimal denomination contract was introduced.

Use the browser UltraWallet flow for ordinary transfers. The one-time genesis
supply correction has a separate version-4 offline workflow documented in
OFFLINE_SUPPLY_CORRECTION.md and must never be produced by this script.
"""

import argparse


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Retired safety guard; no transfer is generated or submitted."
    )
    parser.parse_args()
    parser.exit(
        2,
        "send_ultra.py is retired and intentionally disabled. "
        "Use UltraWallet for normal transfers or the reviewed offline supply-correction CLI.\n",
    )


if __name__ == "__main__":
    raise SystemExit(main())
