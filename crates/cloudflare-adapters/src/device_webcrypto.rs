use device_domain::{DevicePublicKey, DevicePublicKeyAlgorithm};
use worker::js_sys::{Array, Object, Reflect};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::wasm_bindgen_futures::JsFuture;
use worker::web_sys::{CryptoKey, WorkerGlobalScope};
use worker::{Error, Result};

const P256_SPKI_BYTES: usize = 91;
const P256_POINT_OFFSET: usize = 26;
const P256_COORDINATE_BYTES: usize = 32;
const P256_SIGNATURE_BYTES: usize = 64;

/// Verify an IEEE-P1363 `r || s` ECDSA P-256/SHA-256 proof with the exact public key persisted by
/// the device binding authority. Importing the projected EC JWK through WebCrypto is intentional:
/// unlike the domain transport-shape check, WebCrypto rejects points that are not members of P-256.
pub async fn verify_p256_sha256(
    public_key: &DevicePublicKey,
    signature: &[u8],
    message: &[u8],
) -> Result<bool> {
    if public_key.algorithm() != DevicePublicKeyAlgorithm::EcdsaP256Sha256
        || signature.len() != P256_SIGNATURE_BYTES
        || message.is_empty()
    {
        return Ok(false);
    }

    let global: WorkerGlobalScope = worker::js_sys::global().unchecked_into();
    let subtle = global.crypto()?.subtle();
    let jwk = p256_jwk_object(public_key)?;
    let import_algorithm = p256_import_algorithm()?;
    let usages = Array::new();
    usages.push(&JsValue::from_str("verify"));
    let import_promise = subtle
        .import_key_with_object("jwk", &jwk, &import_algorithm, false, usages.as_ref())
        .map_err(js_error)?;
    let imported = match JsFuture::from(import_promise).await {
        Ok(value) => value,
        // Invalid/off-curve public points are an authorization rejection, not a Worker failure.
        Err(_) => return Ok(false),
    };
    let crypto_key: CryptoKey = imported.dyn_into().map_err(js_error)?;

    let verify_algorithm = p256_verify_algorithm()?;
    let verify_promise = subtle
        .verify_with_object_and_u8_array_and_u8_array(
            &verify_algorithm,
            &crypto_key,
            signature,
            message,
        )
        .map_err(js_error)?;
    JsFuture::from(verify_promise)
        .await
        .map_err(js_error)?
        .as_bool()
        .ok_or_else(|| Error::RustError("WebCrypto verification returned a non-boolean".to_owned()))
}

fn p256_jwk_object(public_key: &DevicePublicKey) -> Result<Object> {
    let der = public_key.spki_der();
    if der.len() != P256_SPKI_BYTES || der[P256_POINT_OFFSET] != 0x04 {
        return Err(Error::RustError(
            "P-256 public key transport shape is invalid".to_owned(),
        ));
    }
    let x_start = P256_POINT_OFFSET + 1;
    let y_start = x_start + P256_COORDINATE_BYTES;
    let y_end = y_start + P256_COORDINATE_BYTES;

    let jwk = Object::new();
    set_string(&jwk, "kty", "EC")?;
    set_string(&jwk, "crv", "P-256")?;
    set_string(&jwk, "x", &base64url_no_pad(&der[x_start..y_start]))?;
    set_string(&jwk, "y", &base64url_no_pad(&der[y_start..y_end]))?;
    Reflect::set(&jwk, &JsValue::from_str("ext"), &JsValue::TRUE)
        .map(|_| ())
        .map_err(js_error)?;
    Ok(jwk)
}

fn p256_import_algorithm() -> Result<Object> {
    let algorithm = Object::new();
    set_string(&algorithm, "name", "ECDSA")?;
    set_string(&algorithm, "namedCurve", "P-256")?;
    Ok(algorithm)
}

fn p256_verify_algorithm() -> Result<Object> {
    let algorithm = Object::new();
    set_string(&algorithm, "name", "ECDSA")?;
    set_string(&algorithm, "hash", "SHA-256")?;
    Ok(algorithm)
}

fn set_string(target: &Object, name: &str, value: &str) -> Result<()> {
    Reflect::set(target, &JsValue::from_str(name), &JsValue::from_str(value))
        .map(|_| ())
        .map_err(js_error)
}

fn base64url_no_pad(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk[1]) << 8)
            | u32::from(chunk[2]);
        output.push(char::from(TABLE[((value >> 18) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 12) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 6) & 0x3f) as usize]));
        output.push(char::from(TABLE[(value & 0x3f) as usize]));
    }
    let remainder = chunks.remainder();
    if remainder.len() == 1 {
        let value = u32::from(remainder[0]) << 16;
        output.push(char::from(TABLE[((value >> 18) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 12) & 0x3f) as usize]));
    } else if remainder.len() == 2 {
        let value = (u32::from(remainder[0]) << 16) | (u32::from(remainder[1]) << 8);
        output.push(char::from(TABLE[((value >> 18) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 12) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 6) & 0x3f) as usize]));
    }
    output
}

fn js_error(value: JsValue) -> Error {
    Error::JsError(
        value
            .as_string()
            .unwrap_or_else(|| "WebCrypto operation failed".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        P256_COORDINATE_BYTES, P256_POINT_OFFSET, P256_SIGNATURE_BYTES, base64url_no_pad,
    };

    #[test]
    fn p256_transport_offsets_match_uncompressed_spki_shape() {
        assert_eq!(P256_POINT_OFFSET, 26);
        assert_eq!(P256_COORDINATE_BYTES, 32);
        assert_eq!(P256_SIGNATURE_BYTES, 64);
    }

    #[test]
    fn base64url_projection_is_unpadded_and_url_safe() {
        assert_eq!(base64url_no_pad(&[0xff]), "_w");
        assert_eq!(base64url_no_pad(&[0xfb, 0xff]), "-_8");
        assert_eq!(base64url_no_pad(&[0x01, 0x02, 0x03]), "AQID");
        let coordinate = base64url_no_pad(&[0x11; 32]);
        assert_eq!(coordinate.len(), 43);
        assert!(!coordinate.contains('='));
        assert!(!coordinate.contains('+'));
        assert!(!coordinate.contains('/'));
    }
}
