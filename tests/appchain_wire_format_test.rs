// ============================================================
// APPCHAIN WIRE FORMAT — the JSON representation a client must produce
// ============================================================
//
// `AppChainTxRequest` holds three byte fields as `Vec<u8>`: `sender_public_key`, `nullifier` and
// `signature`. Serde encodes those as JSON arrays of integers, so an SDK sends numbers, not hex and not
// base64 — and that encoding is part of the published contract, because a client that sends hex is
// rejected with a message that does not mention the field.
//
// Until this file existed, nothing asserted that encoding. Every other test builds the struct in Rust and
// passes it straight to `appchain_request_to_transaction`, never through JSON. The representation could
// therefore have changed — a field retyped as `String`, or a custom serialiser emitting a string — and
// every client in the wild would have broken while the whole suite stayed green.
//
// An annotation alone would not do it: `serde_json` writes bytes as an array of numbers
// (`serialize_bytes` → `write_byte_array`, serde_json-1.0.150/src/ser.rs:195-199), so
// `#[serde(with = "serde_bytes")]` is wire-compatible. That is why these tests assert the bytes on the
// wire rather than trusting an attribute, and why the contract states the encoding explicitly.
//
// These tests are the pin. They assert the wire format in both directions:
//   * a document with integer arrays parses;
//   * the same document with a hex string in any of those fields does not;
//   * serialising back yields arrays again, so a round trip is stable.

use serde_json::{json, Value};
use UltraNet::api::AppChainTxRequest;

/// Dilithium-5 sizes: `src/faucet/signer.rs:13-15`, `tools/pqclean_dilithium5/pqc/sign/dilithium5.py:20-22`.
const PUBLIC_KEY_BYTES: usize = 2_592;
const SIGNATURE_BYTES: usize = 4_627;
const NULLIFIER_BYTES: usize = 32;

const ADDRESS: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";

/// A document shaped exactly as a wallet posts it, at realistic field sizes, so a length rule cannot be
/// what rejects it later.
fn wire_document() -> Value {
    json!({
        "sender": ADDRESS,
        "sender_public_key": vec![7u8; PUBLIC_KEY_BYTES],
        "nonce": 0,
        "timestamp": 1_800_000_000u64,
        "nullifier": vec![17u8; NULLIFIER_BYTES],
        "fee": 10_000u64,
        "signature": vec![9u8; SIGNATURE_BYTES],
        "version": 5,
        "operation": { "register": { "name": "UltraNet Grid" } }
    })
}

/// The representation the contract specifies: integer arrays, parsed into the same bytes.
#[test]
fn a_document_with_integer_arrays_parses() {
    let text = serde_json::to_string(&wire_document()).expect("serialisable document");
    let request: AppChainTxRequest =
        serde_json::from_str(&text).expect("a client that sends arrays is accepted");

    assert_eq!(request.sender, ADDRESS);
    assert_eq!(request.sender_public_key.len(), PUBLIC_KEY_BYTES);
    assert_eq!(request.nullifier.len(), NULLIFIER_BYTES);
    assert_eq!(request.signature.len(), SIGNATURE_BYTES);
    assert_eq!(request.nullifier, vec![17u8; NULLIFIER_BYTES]);
    assert_eq!(request.version, 5);
    assert!(matches!(
        request.operation,
        UltraNet::api::AppChainOperation::Register { .. }
    ));
}

/// Hex and base64 are the two encodings a client is most likely to reach for, and both are wrong. The
/// strings below have the correct byte length decoded, so the only defect is the type — which is what
/// makes this a test of the contract rather than of a length check.
#[test]
fn a_string_in_a_byte_field_is_rejected() {
    let cases = [
        ("sender_public_key", "ab".repeat(PUBLIC_KEY_BYTES)),
        ("nullifier", "11".repeat(NULLIFIER_BYTES)),
        ("signature", "cd".repeat(SIGNATURE_BYTES)),
    ];

    for (field, encoded) in cases {
        let mut document = wire_document();
        document[field] = json!(encoded);

        let text = serde_json::to_string(&document).expect("serialisable document");
        let result: Result<AppChainTxRequest, _> = serde_json::from_str(&text);

        let error = match result {
            Ok(_) => panic!("{field} accepted a hex string; the contract says integer arrays"),
            Err(error) => error.to_string(),
        };
        // The node reports this as a sequence-type error, and says nothing about which field: that is
        // exactly why the encoding has to be documented rather than discovered.
        assert!(
            error.contains("sequence"),
            "{field}: rejection does not look like a type error: {error}"
        );
    }
}

/// A round trip must not change the representation: parse, serialise, parse again, and compare. This is
/// the assertion that fails if the field type ever gains a custom serialiser.
#[test]
fn serialising_preserves_the_array_representation() {
    let text = serde_json::to_string(&wire_document()).expect("serialisable document");
    let request: AppChainTxRequest = serde_json::from_str(&text).expect("parses");
    let encoded = serde_json::to_value(&request).expect("reserialisable");

    for (field, expected_len) in [
        ("sender_public_key", PUBLIC_KEY_BYTES),
        ("nullifier", NULLIFIER_BYTES),
        ("signature", SIGNATURE_BYTES),
    ] {
        let array = encoded[field]
            .as_array()
            .unwrap_or_else(|| panic!("{field} is no longer a JSON array after serialisation"));
        assert_eq!(array.len(), expected_len, "{field} changed length on round trip");
        assert!(
            array.iter().all(|element| element.as_u64().is_some()),
            "{field} contains something that is not an unsigned number"
        );
    }

    // And the whole document is stable through a second trip.
    let again: AppChainTxRequest =
        serde_json::from_value(encoded.clone()).expect("re-parses after serialisation");
    let reencoded = serde_json::to_value(&again).expect("reserialisable");
    assert_eq!(encoded, reencoded, "the wire representation is not stable across a round trip");
}
