#!/usr/bin/env python3
"""Reject newly localized text in operator-visible Rust output."""

from pathlib import Path
import re
import sys

DIACRITICS = re.compile(r"[čćžšđČĆŽŠĐ]")
DISPLAY_MARKERS = (
    "println!(",
    "eprintln!(",
    "message:",
    "return Err(",
    "Err(",
    "format!(",
    "panic!(",
)
LOCALIZED_TERMS = re.compile(
    r"(?:Nedovoljno|Stanje|Potrebno|prenizak|Validatori|dodao|Vrijeme|Anchor je|"
    r"Lider je|reputacija|Broj komitovanih|Ukupno|Komitovano|Validacija|"
    r"Greška|greška|Pritisni|pokrenut|Rudari|Balans|Statistika|Kreir|"
    r"Provera|Proveri|Učit|Ažurir|Očisti|Generiši|Pokreć|Dodat|"
    r"Nedostaje|Nevalidan|uspešan|neuspešan)"
)
SOURCE_ROOT = Path(__file__).resolve().parents[1] / "src"


def main() -> int:
    violations: list[str] = []
    for path in sorted(SOURCE_ROOT.rglob("*.rs")):
        in_display_expression = False
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            stripped = line.lstrip()
            if stripped.startswith("//"):
                continue

            if any(marker in line for marker in DISPLAY_MARKERS):
                in_display_expression = True
            if in_display_expression and (
                DIACRITICS.search(line) or LOCALIZED_TERMS.search(line)
            ):
                violations.append(f"{path}:{line_number}: {line.strip()}")
            if in_display_expression and ");" in line:
                in_display_expression = False

    if violations:
        print("Localized characters found in operator-visible Rust output:")
        print("\n".join(violations))
        return 1

    print("English runtime output check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
