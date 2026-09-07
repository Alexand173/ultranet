#!/usr/bin/env python3
"""Offline tests for the staging faucet egress policy helper."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest


MODULE_PATH = Path(__file__).with_name("ultranet-faucet-staging-egress.py")
SPEC = importlib.util.spec_from_file_location("staging_egress", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class EgressPolicyTests(unittest.TestCase):
    def test_valid_public_ranges_are_accepted(self) -> None:
        v4 = "\n".join(f"{11 + index}.0.0.0/16" for index in range(10))
        v6 = "\n".join(f"2001:4860:4860:{index:x}::/64" for index in range(5))
        self.assertEqual(len(MODULE.validate_ranges(v4, 4)), 10)
        self.assertEqual(len(MODULE.validate_ranges(v6, 6)), 5)

    def test_default_private_and_overlapping_ranges_are_rejected(self) -> None:
        with self.assertRaises(MODULE.PolicyError):
            MODULE.validate_ranges("0.0.0.0/0\n" + "\n".join(f"{11 + index}.0.0.0/16" for index in range(10)), 4)
        with self.assertRaises(MODULE.PolicyError):
            MODULE.validate_ranges("10.0.0.0/8\n" + "\n".join(f"{11 + index}.0.0.0/16" for index in range(10)), 4)
        with self.assertRaises(MODULE.PolicyError):
            MODULE.validate_ranges("11.0.0.0/16\n11.0.0.0/24\n" + "\n".join(f"{12 + index}.0.0.0/16" for index in range(9)), 4)

    def test_dropin_is_exactly_an_ipaddress_allowlist(self) -> None:
        allowlist = ["127.0.0.0/8", "::1/128", "11.0.0.0/16", "2001:4860:4860::/64"]
        rendered = MODULE.make_dropin(allowlist).decode("ascii")
        self.assertEqual(MODULE.parse_generated_dropin(rendered), sorted(allowlist, key=lambda value: (MODULE.ipaddress.ip_network(value).version, int(MODULE.ipaddress.ip_network(value).network_address), value)))
        with self.assertRaises(MODULE.PolicyError):
            MODULE.parse_generated_dropin(rendered + "IPAddressDeny=any\n")
        with self.assertRaises(MODULE.PolicyError):
            MODULE.parse_generated_dropin(rendered + "\n[Unit]\nDescription=unexpected\n")

    def test_turnstile_hostname_is_not_an_allowlist_source(self) -> None:
        ranges = MODULE.validate_ranges("\n".join(f"{11 + index}.0.0.0/16" for index in range(10)), 4)
        ranges6 = MODULE.validate_ranges("\n".join(f"2001:4860:4860:{index:x}::/64" for index in range(5)), 6)
        allowlist = MODULE.make_allowlist(ranges, ranges6, [MODULE.ipaddress.ip_address("127.0.0.53")])
        self.assertNotIn(MODULE.TURNSTILE_HOSTNAME, allowlist)
        self.assertIn("127.0.0.53/32", allowlist)

    def test_resolver_allowlist_is_rendered_as_ascii_lines(self) -> None:
        addresses = [
            MODULE.ipaddress.ip_address("127.0.0.53"),
            MODULE.ipaddress.ip_address("2001:4860:4860::8888"),
        ]
        self.assertEqual(
            MODULE.render_resolver_allowlist(addresses),
            b"127.0.0.53\n2001:4860:4860::8888\n",
        )


if __name__ == "__main__":
    unittest.main()
