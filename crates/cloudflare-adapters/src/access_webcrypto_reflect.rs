use crate::access_identity::PreparedAccessJwt;
use serde::Deserialize;
use worker::js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::wasm_bindgen_futures::JsFuture;
use worker::{Error, Result};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AccessJwks {
    pub keys: Vec<AccessRsaJwk>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AccessRsaJwk {
    pub kid: String,
    pub kty: String,
    pub alg: String,
    #[serde(rename = "use")]
    pub usage: Option<String>,
    pub n: String,
    pub e: String,
}

impl AccessJwks {
    #[must_use]
    pub fn matching_key(&self, key_id: &str) -> Option<&AccessRsaJwk> {
        self.keys.iter().find(|key| {
            key.kid == key_id
                && key.kty == "RSA"
                && key.alg == "RS256"
                && key.usage.as_deref().is_none_or(|usage| usage == "sig")
        })
    }
}

pub async fn verify_rs256(
    prepared: &PreparedAccessJwt,
    key: &AccessRsaJwk,
) -> Result<bool> {
    if key.kid != prepared.key_id()
        || key.kty != "RSA"
        || key.alg != "RS256"
        || key.usage.as_deref().is_some_and(|usage| usage != "sig")
    {
        return Ok(false);
    }

    let global = worker::js_sys::global();
    let crypto = Reflect::get(&global, &JsValue::from_str("crypto")).map_err(js_error)?;
    let subtle = Reflect::get(&crypto, &JsValue::from_str("subtle")).map_err(js_error)?;

    let import_key: Function = Reflect::get(&subtle, &JsValue::from_str("importKey"))
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let usages = Array::new();
    usages.push(&JsValue::from_str("verify"));
    let import_arguments = Array::new();
    import_arguments.push(&JsValue::from_str("jwk"));
    import_arguments.push(&rsa_jwk_object(key)?);
    import_arguments.push(&rsa_algorithm_object()?);
    import_arguments.push(&JsValue::from_bool(false));
    import_arguments.push(&usages);
    let import_promise: Promise = import_key
        .apply(&subtle, &import_arguments)
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let crypto_key = JsFuture::from(import_promise).await.map_err(js_error)?;

    let verify: Function = Reflect::get(&subtle, &JsValue::from_str("verify"))
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let signature = Uint8Array::from(prepared.signature());
    let signing_input = Uint8Array::from(prepared.signing_input().as_bytes());
    let verify_arguments = Array::new();
    verify_arguments.push(&JsValue::from_str("RSASSA-PKCS1-v1_5"));
    verify_arguments.push(&crypto_key);
    verify_arguments.push(&signature);
    verify_arguments.push(&signing_input);
    let verify_promise: Promise = verify
        .apply(&subtle, &verify_arguments)
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    JsFuture::from(verify_promise)
        .await
        .map_err(js_error)?
        .as_bool()
        .ok_or_else(|| Error::RustError("WebCrypto verification returned a non-boolean".to_owned()))
}

fn rsa_jwk_object(key: &AccessRsaJwk) -> Result<Object> {
    let jwk = Object::new();
    set_string(&jwk, "kid", &key.kid)?;
    set_string(&jwk, "kty", &key.kty)?;
    set_string(&jwk, "alg", &key.alg)?;
    set_string(&jwk, "use", key.usage.as_deref().unwrap_or("sig"))?;
    set_string(&jwk, "n", &key.n)?;
    set_string(&jwk, "e", &key.e)?;
    Ok(jwk)
}

fn rsa_algorithm_object() -> Result<Object> {
    let algorithm = Object::new();
    set_string(&algorithm, "name", "RSASSA-PKCS1-v1_5")?;
    set_string(&algorithm, "hash", "SHA-256")?;
    Ok(algorithm)
}

fn set_string(target: &Object, name: &str, value: &str) -> Result<()> {
    Reflect::set(
        target,
        &JsValue::from_str(name),
        &JsValue::from_str(value),
    )
    .map(|_| ())
    .map_err(js_error)
}

fn js_error(value: JsValue) -> Error {
    Error::JsError(value)
}

#[cfg(test)]
mod tests {
    use super::{AccessJwks, AccessRsaJwk};

    #[test]
    fn only_matching_rs256_signature_key_is_selected() {
        let keys = AccessJwks {
            keys: vec![
                AccessRsaJwk {
                    kid: "wrong".to_owned(),
                    kty: "RSA".to_owned(),
                    alg: "RS256".to_owned(),
                    usage: Some("sig".to_owned()),
                    n: "n".to_owned(),
                    e: "AQAB".to_owned(),
                },
                AccessRsaJwk {
                    kid: "key-01".to_owned(),
                    kty: "RSA".to_owned(),
                    alg: "RS256".to_owned(),
                    usage: Some("sig".to_owned()),
                    n: "n".to_owned(),
                    e: "AQAB".to_owned(),
                },
            ],
        };
        assert_eq!(
            keys.matching_key("key-01").map(|key| key.kid.as_str()),
            Some("key-01")
        );
        assert!(keys.matching_key("missing").is_none());
    }
}
