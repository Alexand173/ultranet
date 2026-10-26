# `pqc` — PQClean Dilithium-5 binding for the transfer scripts

`send_ultra.py` and `send_ultra_staging.py` sign Sovereign transactions from Python
(`from pqc.sign import dilithium5`). This directory builds that module from **this repository's own**
vendored PQClean sources, so nothing is downloaded and the signatures match the chain.

```sh
tools/pqclean_dilithium5/build.sh            # installs into ./venv/lib/python3.14/site-packages
./venv/bin/python send_ultra.py              # dry run
```

`build.sh` prints a self-test result: a signature verifies and a tampered message does not.

## Why this and not `pqcrypto`

The node signs with the `pqcrypto-dilithium 0.5` crate (`Cargo.toml`), and
`website/crypto/ultra-dilithium5/pqclean/` is a copy of those same sources — the website and the
Android app compile them too. This binding links that C directly through
`website/crypto/ultra-dilithium5/ultra_dilithium5.c`, so a signature made here is byte-identical to
one made by the node.

The `pqcrypto` 1.0.0 package installed in `./venv` exposes **ML-DSA** (`ml_dsa_87`), which is a
different scheme: FIPS 204 changed the signing preimage, so an ML-DSA signature does not verify against
a Dilithium-5 public key. Using it would produce transactions the node rejects, so it is deliberately
not used here.

## Verifying a signature independently

`send_ultra.py --check-file <path>` writes the digest, one signature and the public key. The example
below verifies them with the node's own crypto:

```sh
./venv/bin/python send_ultra.py --check-file /tmp/sig-artifact.json
cargo run --locked --example verify_sig_artifact -- /tmp/sig-artifact.json
```

Expected: `node verifies : true`, with `derived address` equal to a Sovereign owner address.

## Provenance

The C sources are unmodified PQClean Dilithium-5 (CC0), as recorded in
`website/crypto/ultra-dilithium5/NOTICE.txt`. Only the Python wrapper in `pqc/` is new; it is
repository code under the same licence as the rest of the repo. Build output stays in `build/` and the
package is copied into the virtualenv, so neither is tracked by git.
