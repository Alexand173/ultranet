# UltraNet CLI authentication signing

The `ultranet-auth` binary creates a Dilithium-5 authentication payload without UltraWallet. Private sovereign keys remain on the offline signing machine. The CLI never sends a private key to the node.

## Build

```bash
cargo build --release --locked --bin ultranet-auth
```

The binary is written to `target/release/ultranet-auth`.

## Sign a fresh challenge

The CLI selects a key record from a local key file, derives its 64-character node identifier, requests a short-lived challenge, signs the canonical challenge locally, and prints the exact JSON body expected by `POST /api/auth/login`. It accepts the generated `sovereign_keys.json` format (hex strings under either a top-level array or `owners`) and the owner backup format used by `owner1_key.json` (byte arrays under `owners`, with `private_key`):

```bash
./target/release/ultranet-auth sign-challenge \
  --api-base-url https://api.ultranetwork.cc \
  --keys /offline/sovereign_keys.json \
  --key-index 0 \
  --output /offline/auth-login-payload.json
```

`--key-index` is zero-based. Keep the local key file restricted (`chmod 600 /offline/sovereign_keys.json`). The output file is created only if it does not already exist and is restricted to mode `0600` on Unix systems. It contains only the challenge fields, public key bytes, and signature bytes; it does not contain the private key. The CLI also self-checks that the selected public and secret key bytes form the same Dilithium-5 keypair before requesting a network challenge.

For local development, omit `--api-base-url` to use `http://127.0.0.1:8081`, or set `ULTRANET_API_BASE_URL`. Use an HTTPS API origin for production.

## API submission

The generated file is an API login request. A CLI client can submit it with a cookie jar:

```bash
curl --fail-with-body \
  -c /offline/ultranet-cookies.txt \
  -b /offline/ultranet-cookies.txt \
  -H 'Content-Type: application/json' \
  --data-binary @/offline/auth-login-payload.json \
  https://api.ultranetwork.cc/api/auth/login
```

The response sets the `ultranet_session` HttpOnly cookie and the readable `ultranet_csrf` cookie. Keep the cookie jar private. The CLI payload is single-use because the node consumes the challenge after a successful login.

## Browser import

The `/login` page supports two authentication methods: UltraWallet signing and CLI signed-payload import. To use the CLI path:

1. Generate a fresh payload with `ultranet-auth` on the offline signing machine.
2. Open the deployed `/login` page and choose `CLI_SIGNED_PAYLOAD`.
3. Paste the JSON object from `auth-login-payload.json` into the payload field.
4. Select `IMPORT_SIGNED_PAYLOAD` and wait for the session redirect.

The browser accepts only the seven public login fields required by `POST /api/auth/login`. It rejects unknown fields such as `secret_key`, `private_key`, and sovereign key-file data before making a request. The pasted payload is held in page memory only, is never saved to browser storage or URLs, and is cleared after a successful login. Payloads remain short-lived and single-use; generate a fresh payload after an expiry or failed consumption attempt.

Do not:

- copy `sovereign_keys.json` or any private key to the VPS;
- put private keys in frontend source, browser storage, URLs, logs, or API request bodies;
- reuse an expired or already-consumed challenge;
- use the administrator bearer token as a replacement for wallet authentication.
