# UltraNet Dilithium-5 browser module

This directory contains the small C wrapper used to build the checked-in
`public/crypto/ultra-dilithium5.wasm` module.

The algorithm sources under `pqclean/` are copied from the exact
`pqcrypto-dilithium 0.5.0` PQClean Dilithium-5 clean implementation used by
the Rust node. The source is public domain; see
`pqclean/crypto_sign/dilithium5/clean/LICENSE`.

The wrapper deliberately accepts a 32-byte seed for key generation. The
browser generates that seed with `crypto.getRandomValues()` or derives it
from the validated BIP39 recovery phrase. It exports the native formats:

- public key: 2,592 bytes
- secret key: 4,896 bytes
- detached signature: 4,627 bytes

The module has no operating-system random source. Call `ultra_keypair` with a
fresh 32-byte seed for every generated keypair, and set the exported mutable
`__stack_pointer` to a safe value before invoking crypto routines. The website
loader owns allocation, serializes calls, resets the scratch heap, and wipes
input/output regions after each operation.

## Rebuild

The checked-in artifact is reproducible with clang targeting wasm32 and the
Rust toolchain's `wasm-ld`:

```bash
./build.sh
```

Run the cross-language fixture after rebuilding. A signature is not
compatible merely because a package calls itself “Dilithium5”; the 4,627-byte
signature and Rust verification test are release gates.
