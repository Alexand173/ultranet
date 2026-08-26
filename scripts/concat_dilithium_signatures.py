#!/usr/bin/env python3
"""Concatenate two hexadecimal Dilithium-5 signatures for UltraNet approval.

Each input file must contain one hexadecimal Dilithium-5 signature. Whitespace
and an optional leading ``0x`` are ignored. The script validates that each
signature decodes to 4,627 bytes and writes one flat JSON array containing
9,254 integer byte values:

    python3 scripts/concat_dilithium_signatures.py owner1.sig.hex owner2.sig.hex \
        --output combined-signature.json

Use ``--as-payload`` when the result should be directly usable as the
``signature`` field of the approval request. The script never prints the input
hex strings or any key material.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import TextIO

SIGNATURE_BYTES = 4_627
COMBINED_SIGNATURE_BYTES = SIGNATURE_BYTES * 2


def load_signature(path: Path, owner_label: str) -> list[int]:
    """Read and validate one hexadecimal Dilithium-5 signature."""
    try:
        raw = path.read_text(encoding="ascii")
    except OSError as exc:
        raise ValueError(f"cannot read {owner_label} signature file {path}: {exc}") from exc
    except UnicodeDecodeError as exc:
        raise ValueError(f"{owner_label} signature file {path} is not ASCII hex") from exc

    value = "".join(raw.split())
    if value.lower().startswith("0x"):
        value = value[2:]

    if not value:
        raise ValueError(f"{owner_label} signature file {path} is empty")
    if len(value) % 2:
        raise ValueError(f"{owner_label} signature hex must contain an even number of characters")

    try:
        signature = bytes.fromhex(value)
    except ValueError as exc:
        raise ValueError(f"{owner_label} signature file {path} contains non-hex characters") from exc

    if len(signature) != SIGNATURE_BYTES:
        raise ValueError(
            f"{owner_label} signature must decode to exactly {SIGNATURE_BYTES} bytes; "
            f"received {len(signature)} bytes"
        )

    return list(signature)


def write_json(value: object, output: TextIO, pretty: bool) -> None:
    """Write JSON without exposing any intermediate signing material."""
    json.dump(value, output, indent=2 if pretty else None, separators=None if pretty else (",", ":"))
    output.write("\n")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Validate and concatenate two 4,627-byte hexadecimal Dilithium-5 "
            "signatures into one flat 9,254-integer JSON array."
        )
    )
    parser.add_argument("owner1_signature", type=Path, help="file containing owner 1's hexadecimal signature")
    parser.add_argument("owner2_signature", type=Path, help="file containing owner 2's hexadecimal signature")
    parser.add_argument("-o", "--output", type=Path, help="write JSON to this file instead of stdout")
    parser.add_argument("--pretty", action="store_true", help="pretty-print the JSON output")
    parser.add_argument(
        "--as-payload",
        action="store_true",
        help='wrap the result as {"signature": [...]} instead of emitting the bare array',
    )
    return parser


def main() -> int:
    args = build_parser().parse_args()

    try:
        signature1 = load_signature(args.owner1_signature, "owner 1")
        signature2 = load_signature(args.owner2_signature, "owner 2")
        combined = signature1 + signature2
        if len(combined) != COMBINED_SIGNATURE_BYTES:
            raise ValueError(
                f"combined signature must contain exactly {COMBINED_SIGNATURE_BYTES} bytes; "
                f"received {len(combined)} bytes"
            )

        value: object = {"signature": combined} if args.as_payload else combined
        if args.output:
            with args.output.open("w", encoding="utf-8", newline="\n") as stream:
                write_json(value, stream, args.pretty)
        else:
            write_json(value, sys.stdout, args.pretty)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    except OSError as exc:
        print(f"error: cannot write output: {exc}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
