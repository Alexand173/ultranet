#!/usr/bin/env bash
# Build the `pqc` Python binding (Dilithium5) from this repository's own PQClean sources, so the
# transfer scripts (send_ultra.py, send_ultra_staging.py) can run locally.
#
# Usage: tools/pqclean_dilithium5/build.sh [install-dir]
#   install-dir defaults to <repo>/venv/lib/python3.14/site-packages
#   PYTHON and CC may be overridden in the environment.
#
# The C sources are the same ones the Rust node, the website and the Android app compile, so a
# signature produced here verifies on the chain. Nothing is downloaded.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC="${ROOT}/website/crypto/ultra-dilithium5"
CLEAN="${SRC}/pqclean/crypto_sign/dilithium5/clean"
COMMON="${SRC}/pqclean/common"
PKG="${ROOT}/tools/pqclean_dilithium5/pqc"
BUILD="${ROOT}/tools/pqclean_dilithium5/build"
CC="${CC:-gcc}"
TARGET="${1:-${ROOT}/venv/lib/python3.14/site-packages}"
PYTHON="${PYTHON:-${ROOT}/venv/bin/python}"

if [[ ! -d "${CLEAN}" ]]; then
  echo "PQClean Dilithium5 sources are missing at ${CLEAN}" >&2
  exit 1
fi
if ! command -v "${CC}" >/dev/null; then
  echo "C compiler '${CC}' not found; set CC to one that exists." >&2
  exit 1
fi

echo "Compiling Dilithium-5 from ${SRC} with ${CC}"
mkdir -p "${BUILD}" "${TARGET}/pqc/sign"

# -iquote, not -I: $SRC also holds the wasm shim headers (unistd.h, string.h, stdlib.h) which must
# not shadow the host's <system headers>. Only the quoted "pqclean/..." include needs $SRC.
CFLAGS=(-O2 -fPIC -std=c99 -Wall -fno-strict-aliasing -iquote "${SRC}" -I"${COMMON}" -I"${CLEAN}")

objects=()
for source in \
  "${SRC}/ultra_dilithium5.c" \
  "${SRC}/randombytes.c" \
  "${SRC}/abort.c" \
  "${COMMON}/fips202.c" \
  "${CLEAN}/ntt.c" \
  "${CLEAN}/packing.c" \
  "${CLEAN}/poly.c" \
  "${CLEAN}/polyvec.c" \
  "${CLEAN}/reduce.c" \
  "${CLEAN}/rounding.c" \
  "${CLEAN}/sign.c" \
  "${CLEAN}/symmetric-shake.c"; do
  object="${BUILD}/$(basename "${source}" .c).o"
  "${CC}" "${CFLAGS[@]}" -c "${source}" -o "${object}"
  objects+=("${object}")
done

"${CC}" -shared -o "${BUILD}/_ultra_dilithium5.so" "${objects[@]}"
echo "Built ${BUILD}/_ultra_dilithium5.so"

install -m 0644 "${PKG}/__init__.py" "${TARGET}/pqc/__init__.py"
install -m 0644 "${PKG}/sign/__init__.py" "${TARGET}/pqc/sign/__init__.py"
install -m 0644 "${PKG}/sign/dilithium5.py" "${TARGET}/pqc/sign/dilithium5.py"
install -m 0755 "${BUILD}/_ultra_dilithium5.so" "${TARGET}/pqc/sign/_ultra_dilithium5.so"
echo "Installed pqc into ${TARGET}"

# Self-test: a signature verifies, and a tampered message does not.
"${PYTHON}" - <<'PY'
from hashlib import sha3_256
from pqc.sign import dilithium5

seed = bytes(range(32))
public_key, secret_key = dilithium5.keypair(seed)
assert len(public_key) == dilithium5.PUBLIC_KEY_BYTES
assert len(secret_key) == dilithium5.SECRET_KEY_BYTES
digest = sha3_256(b"ultranet pqc binding self-test").digest()
signature = dilithium5.sign(digest, secret_key)
assert len(signature) == dilithium5.SIGNATURE_BYTES
assert dilithium5.verify(signature, digest, public_key)
assert not dilithium5.verify(signature, sha3_256(b"tampered").digest(), public_key)
print("self-test: keypair, sign and verify OK")
PY
echo "Run the transfer scripts with: ${PYTHON} send_ultra.py"
