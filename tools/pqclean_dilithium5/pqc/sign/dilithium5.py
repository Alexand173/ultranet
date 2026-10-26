"""ctypes binding to the repository's vendored PQClean Dilithium-5.

The node signs and verifies with the PQClean clean implementation compiled from
`website/crypto/ultra-dilithium5/pqclean`, and the website and Android app compile the same sources.
This module links the very same C through `ultra_dilithium5.c`, so signatures produced here verify on
the chain with no encoding differences.

It is deliberately *not* backed by ML-DSA. ML-DSA (FIPS 204) is a different scheme with a different
signing preimage; an `ml_dsa_87` signature will not verify against a Dilithium-5 public key, so using
it would produce transactions the node rejects.

Build and install with `tools/pqclean_dilithium5/build.sh`.
"""

from __future__ import annotations

import ctypes
from pathlib import Path

PUBLIC_KEY_BYTES = 2592
SECRET_KEY_BYTES = 4896
SIGNATURE_BYTES = 4627
SEED_BYTES = 32

_LIBRARY = Path(__file__).with_name("_ultra_dilithium5.so")
if not _LIBRARY.exists():
    raise ImportError(
        f"{_LIBRARY} is missing. Run tools/pqclean_dilithium5/build.sh to compile it from the "
        "repository's PQClean sources."
    )

_LIB = ctypes.CDLL(str(_LIBRARY))

_U8P = ctypes.POINTER(ctypes.c_uint8)

_LIB.ultra_keypair.argtypes = [_U8P, _U8P, _U8P]
_LIB.ultra_keypair.restype = ctypes.c_int
_LIB.ultra_sign.argtypes = [_U8P, ctypes.c_size_t, _U8P, _U8P]
_LIB.ultra_sign.restype = ctypes.c_int
_LIB.ultra_verify.argtypes = [_U8P, ctypes.c_size_t, _U8P, ctypes.c_size_t, _U8P]
_LIB.ultra_verify.restype = ctypes.c_int


def _input(data: bytes) -> tuple[ctypes.Array, _U8P]:
    """A binary-safe ctypes buffer. `c_char_p` would stop at the first NUL byte."""
    array = (ctypes.c_uint8 * max(len(data), 1)).from_buffer_copy(bytes(data) or b"\x00")
    return array, ctypes.cast(array, _U8P)


def _output(size: int) -> tuple[ctypes.Array, _U8P]:
    array = (ctypes.c_uint8 * size)()
    return array, ctypes.cast(array, _U8P)


def keypair(seed: bytes) -> tuple[bytes, bytes]:
    """Derive `(public_key, secret_key)` from a 32-byte seed, as the wallet and node do."""
    if len(seed) != SEED_BYTES:
        raise ValueError(f"a Dilithium-5 seed is {SEED_BYTES} bytes, got {len(seed)}")
    seed_array, seed_ptr = _input(seed)
    public_array, public_ptr = _output(PUBLIC_KEY_BYTES)
    secret_array, secret_ptr = _output(SECRET_KEY_BYTES)
    try:
        if _LIB.ultra_keypair(seed_ptr, public_ptr, secret_ptr) != 0:
            raise ValueError("Dilithium-5 key generation failed")
        return bytes(public_array), bytes(secret_array)
    finally:
        ctypes.memset(seed_ptr, 0, len(seed_array))
        ctypes.memset(secret_ptr, 0, SECRET_KEY_BYTES)


def sign(message: bytes, secret_key: bytes) -> bytes:
    """Sign raw bytes. The node passes the 32-byte SHA3-256 transaction digest as `message`."""
    if len(secret_key) != SECRET_KEY_BYTES:
        raise ValueError(f"a Dilithium-5 secret key is {SECRET_KEY_BYTES} bytes, got {len(secret_key)}")
    message_array, message_ptr = _input(message)
    key_array, key_ptr = _input(secret_key)
    signature_array, signature_ptr = _output(SIGNATURE_BYTES)
    try:
        if _LIB.ultra_sign(message_ptr, ctypes.c_size_t(len(message)), key_ptr, signature_ptr) != 0:
            raise ValueError("Dilithium-5 signing failed")
        return bytes(signature_array)
    finally:
        ctypes.memset(key_ptr, 0, len(key_array))


def verify(signature: bytes, message: bytes, public_key: bytes) -> bool:
    """Verify one 4627-byte signature over `message` under `public_key`."""
    if len(signature) != SIGNATURE_BYTES:
        raise ValueError(f"a Dilithium-5 signature is {SIGNATURE_BYTES} bytes, got {len(signature)}")
    if len(public_key) != PUBLIC_KEY_BYTES:
        raise ValueError(f"a Dilithium-5 public key is {PUBLIC_KEY_BYTES} bytes, got {len(public_key)}")
    signature_array, signature_ptr = _input(signature)
    message_array, message_ptr = _input(message)
    public_array, public_ptr = _input(public_key)
    return _LIB.ultra_verify(
        signature_ptr,
        ctypes.c_size_t(len(signature)),
        message_ptr,
        ctypes.c_size_t(len(message)),
        public_ptr,
    ) == 0
