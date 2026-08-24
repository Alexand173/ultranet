#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="${ROOT}/../../public/crypto/ultra-dilithium5.wasm"
BUILD="${ROOT}/build"
mkdir -p "$BUILD" "$(dirname "$OUT")"

for source in \
  mini_libc.c randombytes.c ultra_dilithium5.c abort.c \
  pqclean/common/fips202.c \
  pqclean/crypto_sign/dilithium5/clean/ntt.c \
  pqclean/crypto_sign/dilithium5/clean/packing.c \
  pqclean/crypto_sign/dilithium5/clean/poly.c \
  pqclean/crypto_sign/dilithium5/clean/polyvec.c \
  pqclean/crypto_sign/dilithium5/clean/reduce.c \
  pqclean/crypto_sign/dilithium5/clean/rounding.c \
  pqclean/crypto_sign/dilithium5/clean/sign.c \
  pqclean/crypto_sign/dilithium5/clean/symmetric-shake.c; do
  object="$BUILD/$(basename "$source" .c).o"
  clang --target=wasm32 -O3 -fno-builtin -nostdlib \
    -I"$ROOT" -I"$ROOT/pqclean/common" \
    -I"$ROOT/pqclean/crypto_sign/dilithium5/clean" \
    -c "$ROOT/$source" -o "$object"
done

if [[ -z "${WASM_LD:-}" ]]; then
  WASM_LD="$(command -v wasm-ld || true)"
fi
if [[ -z "${WASM_LD:-}" && "$(uname -s)" == "Linux" ]]; then
  WASM_LD="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/gcc-ld/wasm-ld"
fi
if [[ -z "${WASM_LD:-}" || ! -x "$WASM_LD" ]]; then
  echo "A wasm-ld executable is required; set WASM_LD to its path." >&2
  exit 1
fi

clang --target=wasm32 -nostdlib -fuse-ld="$WASM_LD" \
  "$BUILD"/mini_libc.o "$BUILD"/randombytes.o "$BUILD"/ultra_dilithium5.o \
  "$BUILD"/fips202.o "$BUILD"/ntt.o "$BUILD"/packing.o "$BUILD"/poly.o \
  "$BUILD"/polyvec.o "$BUILD"/reduce.o "$BUILD"/rounding.o "$BUILD"/sign.o \
  "$BUILD"/symmetric-shake.o "$BUILD"/abort.o \
  -Wl,--no-entry -Wl,--export-memory -Wl,--stack-first \
  -Wl,--initial-memory=67108864 -Wl,--max-memory=134217728 \
  -Wl,--export=ultra_set_seed -Wl,--export=ultra_public_key_bytes \
  -Wl,--export=ultra_secret_key_bytes -Wl,--export=ultra_signature_bytes \
  -Wl,--export=ultra_keypair -Wl,--export=ultra_sign \
  -Wl,--export=ultra_verify -Wl,--export=malloc -Wl,--export=free \
  -Wl,--export=ultra_reset_heap -Wl,--export=__heap_base \
  -Wl,--export=__stack_pointer -Wl,--strip-all -o "$OUT"

sha256sum "$OUT"
