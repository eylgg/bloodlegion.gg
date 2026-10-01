use base64ct::{Base64UrlUnpadded, Encoding};
use rsa::{
    BoxedUint, RsaPublicKey,
    pkcs1v15::{SigningKey, VerifyingKey},
    sha2::Sha256,
};
use serde::de::DeserializeOwned;
use time::{Duration, OffsetDateTime};

use anyhow::Context;
use thiserror::Error;

use crate::crypto::{generate_signature, verify_signature};
use crate::users::UserId;

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("the token is malformed")]
    Malformed,
    #[error("the token uses an unsupported algorithm")]
    UnsupportedAlgorithm,
    #[error("the verifying key is invalid")]
    InvalidKey,
    #[error("the token signature is invalid")]
    InvalidSignature,
    #[error("the token has expired")]
    Expired,
    #[error("the token issuer does not match")]
    InvalidIssuer,
    #[error("the token audience does not match")]
    InvalidAudience,
}

/// The header we emit on every JWT we sign: always RS256, with `typ`
/// distinguishing an id_token (`JWT`) from an access token (`at+jwt`, RFC 9068)
/// and `kid` naming the active signing key.
#[derive(Debug, serde::Serialize)]
struct JwtHeader<'a> {
    alg: &'static str,
    typ: &'static str,
    kid: &'a str,
}

impl<'a> JwtHeader<'a> {
    fn new(typ: &'static str, kid: &'a str) -> Self {
        Self {
            alg: "RS256",
            typ,
            kid,
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AccessTokenPayload {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub client_id: String,
    pub scope: String,
    pub jti: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, serde::Serialize)]
struct IdTokenPayload<'a> {
    iss: &'a str,
    sub: String,
    aud: &'a str,
    iat: i64,
    exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<&'a str>,
}

pub const ID_TOKEN_LIFETIME: Duration = Duration::minutes(10);

pub const ACCESS_TOKEN_LIFETIME: Duration = Duration::hours(1);

pub fn generate_id_token(
    issuer: &str,
    private_key_id: &str,
    signing_key: &SigningKey<Sha256>,
    user_id: UserId,
    client_id: &str,
    nonce: Option<&str>,
) -> anyhow::Result<String> {
    let now = OffsetDateTime::now_utc();
    let header = JwtHeader::new("JWT", private_key_id);
    let payload = IdTokenPayload {
        iss: issuer,
        sub: user_id.to_string(),
        aud: client_id,
        iat: now.unix_timestamp(),
        exp: (now + ID_TOKEN_LIFETIME).unix_timestamp(),
        nonce,
    };
    generate_signed_jwt(signing_key, &header, &payload)
}

pub fn generate_access_token(
    issuer: &str,
    private_key_id: &str,
    signing_key: &SigningKey<Sha256>,
    jti: &str,
    user_id: UserId,
    client_id: &str,
    scope: &str,
) -> anyhow::Result<String> {
    let now = OffsetDateTime::now_utc();
    let header = JwtHeader::new("at+jwt", private_key_id);
    let payload = AccessTokenPayload {
        iss: issuer.to_string(),
        sub: user_id.to_string(),
        // The audience of an access token is the resource server, which here is
        // this service itself (the userinfo endpoint), so it equals the issuer.
        aud: issuer.to_string(),
        client_id: client_id.to_string(),
        scope: scope.to_string(),
        jti: jti.to_string(),
        iat: now.unix_timestamp(),
        exp: (now + ACCESS_TOKEN_LIFETIME).unix_timestamp(),
    };
    generate_signed_jwt(signing_key, &header, &payload)
}

pub fn validate_access_token(
    verifying_key: &VerifyingKey<Sha256>,
    token: &str,
    expected_issuer: &str,
) -> Result<AccessTokenPayload, JwtError> {
    let (signing_input, signature) = token.rsplit_once('.').ok_or(JwtError::Malformed)?;
    let (header_segment, payload_segment) =
        signing_input.split_once('.').ok_or(JwtError::Malformed)?;
    // Pin the algorithm before trusting anything else, mirroring `verify_rs256`
    // on the relying-party side: the header is attacker-controlled, and only
    // RS256 is ever issued here, so reject anything that claims otherwise rather
    // than relying solely on the key happening to be RSA.
    let header: Header = decode_segment(header_segment)?;
    if header.alg != "RS256" {
        return Err(JwtError::UnsupportedAlgorithm);
    }
    if !verify_signature(verifying_key, signing_input.as_bytes(), signature) {
        return Err(JwtError::InvalidSignature);
    }
    let payload: AccessTokenPayload = decode_segment(payload_segment)?;
    if payload.exp < OffsetDateTime::now_utc().unix_timestamp() {
        return Err(JwtError::Expired);
    }
    // The verifying key is selected by `kid` from our own published JWKS, but that
    // is not by itself proof the token was minted for us: assert the registered
    // `iss`/`aud` (both equal to this origin when we issue an access token), so a
    // token signed by the same key for a different issuer/audience can't be
    // replayed at userinfo. Mirrors `verify_rs256` on the relying-party side.
    if payload.iss != expected_issuer {
        return Err(JwtError::InvalidIssuer);
    }
    if payload.aud != expected_issuer {
        return Err(JwtError::InvalidAudience);
    }
    Ok(payload)
}

fn generate_signed_jwt<H: serde::Serialize, P: serde::Serialize>(
    signing_key: &SigningKey<Sha256>,
    header: &H,
    payload: &P,
) -> anyhow::Result<String> {
    let unsigned = format!(
        "{}.{}",
        Base64UrlUnpadded::encode_string(
            &serde_json::to_vec(header).context("serializing the JWT header")?
        ),
        Base64UrlUnpadded::encode_string(
            &serde_json::to_vec(payload).context("serializing the JWT payload")?
        )
    );
    let signature = generate_signature(signing_key, unsigned.as_bytes());
    Ok(format!("{}.{}", unsigned, signature))
}

/// A JWT header. Only the algorithm and key id matter to us.
#[derive(Debug, serde::Deserialize)]
pub struct Header {
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
}

/// Decodes a JWT's header segment without verifying anything, used to read the
/// `kid` so the matching JWKS key can be selected before verification.
pub fn decode_header(token: &str) -> Result<Header, JwtError> {
    let segment = token.split('.').next().ok_or(JwtError::Malformed)?;
    decode_segment(segment)
}

/// Builds an RS256 verifying key from a JWK's base64url-encoded modulus (`n`)
/// and exponent (`e`).
pub fn rsa_verifying_key(n: &str, e: &str) -> Result<VerifyingKey<Sha256>, JwtError> {
    let modulus = decode_jwk_integer(n)?;
    let exponent = decode_jwk_integer(e)?;
    // n and e are public key material, so a variable-time decode is fine.
    let public_key = RsaPublicKey::new(
        BoxedUint::from_be_slice_vartime(&modulus),
        BoxedUint::from_be_slice_vartime(&exponent),
    )
    .map_err(|_| JwtError::InvalidKey)?;
    Ok(VerifyingKey::<Sha256>::new(public_key))
}

/// Decodes a JWK integer (`n`, `e`) into its minimal big-endian bytes. RFC 7518 requires
/// unpadded base64url with no leading zero octets, but Battle.net publishes `n` in padded standard
/// base64 *with* a leading zero (`AMyHr4r//...Xs=`, which decodes to `0x00 0xcc ...`). Both are
/// tolerated: either alphabet, with or without padding, and leading zeros are stripped. The zeros
/// matter: the RSA arithmetic sizes itself from the byte length, so a 257-byte modulus makes every
/// 256-byte signature fail to verify. Leniency is safe here: this is public key material, and none
/// of it changes the integer's value.
fn decode_jwk_integer(value: &str) -> Result<Vec<u8>, JwtError> {
    let normalized: String = value
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            c => c,
        })
        .collect();
    let bytes = Base64UrlUnpadded::decode_vec(&normalized).map_err(|_| JwtError::InvalidKey)?;
    let first = bytes
        .iter()
        .position(|&byte| byte != 0)
        .unwrap_or(bytes.len());
    Ok(bytes[first..].to_vec())
}

/// The registered claims we validate. `aud` is single- or multi-valued per
/// RFC 7519; `azp` (authorized party) is only required when `aud` is
/// multi-valued (OpenID Connect Core section 3.1.3.7).
#[derive(serde::Deserialize)]
struct RegisteredClaims {
    exp: i64,
    iss: String,
    aud: Audience,
    #[serde(default)]
    azp: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn contains(&self, value: &str) -> bool {
        match self {
            Audience::One(audience) => audience == value,
            Audience::Many(audiences) => audiences.iter().any(|audience| audience == value),
        }
    }
}

/// Verifies an RS256 JWT against `key`, checks the registered `exp`/`iss`/`aud`
/// claims, and deserializes the body into `C`. This is the relying-party side:
/// it verifies id_tokens issued by external OIDC providers.
pub fn verify_rs256<C: DeserializeOwned>(
    token: &str,
    key: &VerifyingKey<Sha256>,
    issuer: &str,
    audience: &str,
) -> Result<C, JwtError> {
    let (signing_input, signature) = token.rsplit_once('.').ok_or(JwtError::Malformed)?;
    let (header_segment, payload_segment) =
        signing_input.split_once('.').ok_or(JwtError::Malformed)?;

    let header: Header = decode_segment(header_segment)?;
    if header.alg != "RS256" {
        return Err(JwtError::UnsupportedAlgorithm);
    }
    if !verify_signature(key, signing_input.as_bytes(), signature) {
        return Err(JwtError::InvalidSignature);
    }

    // Decode the payload bytes once and reuse them for both the registered-claim
    // checks and the caller's typed view.
    let payload =
        Base64UrlUnpadded::decode_vec(payload_segment).map_err(|_| JwtError::Malformed)?;
    let registered: RegisteredClaims =
        serde_json::from_slice(&payload).map_err(|_| JwtError::Malformed)?;
    if registered.exp < OffsetDateTime::now_utc().unix_timestamp() {
        return Err(JwtError::Expired);
    }
    if registered.iss != issuer {
        return Err(JwtError::InvalidIssuer);
    }
    if !registered.aud.contains(audience) {
        return Err(JwtError::InvalidAudience);
    }
    // When the token names more than one audience, OIDC requires an `azp` that
    // identifies the intended client, without it a token minted for a different
    // RP at the same IdP (but co-listing us in `aud`) would verify.
    if let Audience::Many(_) = registered.aud
        && registered.azp.as_deref() != Some(audience)
    {
        return Err(JwtError::InvalidAudience);
    }

    serde_json::from_slice(&payload).map_err(|_| JwtError::Malformed)
}

/// Base64url-decodes a JWT segment and deserializes the JSON it carries.
fn decode_segment<T: DeserializeOwned>(segment: &str) -> Result<T, JwtError> {
    let bytes = Base64UrlUnpadded::decode_vec(segment).map_err(|_| JwtError::Malformed)?;
    serde_json::from_slice(&bytes).map_err(|_| JwtError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::RsaPrivateKey;
    use rsa::traits::PublicKeyParts;

    #[derive(serde::Serialize)]
    struct TestHeader {
        alg: &'static str,
        typ: &'static str,
        kid: &'static str,
    }

    #[derive(serde::Deserialize)]
    struct TestClaims {
        sub: String,
        nonce: Option<String>,
    }

    const ISSUER: &str = "https://idp.example.com";
    const AUDIENCE: &str = "client-123";

    fn header() -> TestHeader {
        TestHeader {
            alg: "RS256",
            typ: "JWT",
            kid: "k1",
        }
    }

    fn claims(exp: i64, sub: &str) -> serde_json::Value {
        serde_json::json!({
            "iss": ISSUER,
            "aud": AUDIENCE,
            "exp": exp,
            "sub": sub,
            "nonce": "nonce-xyz",
        })
    }

    #[test]
    fn verify_rs256_round_trips_through_a_jwk_and_enforces_claims() {
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("generate key");
        let signing_key = SigningKey::<Sha256>::new(private_key.clone());
        let public_key = private_key.to_public_key();

        // Rebuild the verifying key from the JWK components, exercising the real
        // base64url modulus/exponent path the OIDC client uses.
        let key = rsa_verifying_key(
            &Base64UrlUnpadded::encode_string(&public_key.n().to_be_bytes_trimmed_vartime()),
            &Base64UrlUnpadded::encode_string(&public_key.e().to_be_bytes_trimmed_vartime()),
        )
        .expect("build verifying key from jwk");

        let future = (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp();
        let token =
            generate_signed_jwt(&signing_key, &header(), &claims(future, "subject-1")).unwrap();

        let verified: TestClaims = verify_rs256(&token, &key, ISSUER, AUDIENCE).expect("verifies");
        assert_eq!(verified.sub, "subject-1");
        assert_eq!(verified.nonce.as_deref(), Some("nonce-xyz"));

        // Wrong issuer / audience are rejected.
        assert!(matches!(
            verify_rs256::<TestClaims>(&token, &key, "https://evil.example.com", AUDIENCE),
            Err(JwtError::InvalidIssuer)
        ));
        assert!(matches!(
            verify_rs256::<TestClaims>(&token, &key, ISSUER, "other-client"),
            Err(JwtError::InvalidAudience)
        ));

        // A tampered payload no longer matches the signature.
        let (head, rest) = token.split_once('.').unwrap();
        let (_payload, signature) = rest.split_once('.').unwrap();
        let forged = Base64UrlUnpadded::encode_string(
            &serde_json::to_vec(&claims(future, "attacker")).unwrap(),
        );
        let tampered = format!("{head}.{forged}.{signature}");
        assert!(matches!(
            verify_rs256::<TestClaims>(&tampered, &key, ISSUER, AUDIENCE),
            Err(JwtError::InvalidSignature)
        ));

        // An expired token is rejected.
        let past = (OffsetDateTime::now_utc() - Duration::hours(1)).unix_timestamp();
        let expired =
            generate_signed_jwt(&signing_key, &header(), &claims(past, "subject-1")).unwrap();
        assert!(matches!(
            verify_rs256::<TestClaims>(&expired, &key, ISSUER, AUDIENCE),
            Err(JwtError::Expired)
        ));

        // The token does not verify against a different key.
        let other_public = RsaPrivateKey::new(&mut rng, 2048).unwrap().to_public_key();
        let other_key = rsa_verifying_key(
            &Base64UrlUnpadded::encode_string(&other_public.n().to_be_bytes_trimmed_vartime()),
            &Base64UrlUnpadded::encode_string(&other_public.e().to_be_bytes_trimmed_vartime()),
        )
        .unwrap();
        assert!(matches!(
            verify_rs256::<TestClaims>(&token, &other_key, ISSUER, AUDIENCE),
            Err(JwtError::InvalidSignature)
        ));
    }

    #[test]
    fn verify_rs256_requires_azp_for_a_multi_valued_audience() {
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("generate key");
        let signing_key = SigningKey::<Sha256>::new(private_key.clone());
        let public_key = private_key.to_public_key();
        let key = rsa_verifying_key(
            &Base64UrlUnpadded::encode_string(&public_key.n().to_be_bytes_trimmed_vartime()),
            &Base64UrlUnpadded::encode_string(&public_key.e().to_be_bytes_trimmed_vartime()),
        )
        .unwrap();
        let future = (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp();
        let multi_aud = |azp: Option<&str>| {
            let mut claims = serde_json::json!({
                "iss": ISSUER,
                "aud": [AUDIENCE, "another-client"],
                "exp": future,
                "sub": "subject-1",
            });
            if let Some(azp) = azp {
                claims["azp"] = serde_json::json!(azp);
            }
            generate_signed_jwt(&signing_key, &header(), &claims).unwrap()
        };

        // Multi-valued aud with no azp, or an azp naming another client, is rejected.
        assert!(matches!(
            verify_rs256::<TestClaims>(&multi_aud(None), &key, ISSUER, AUDIENCE),
            Err(JwtError::InvalidAudience)
        ));
        assert!(matches!(
            verify_rs256::<TestClaims>(&multi_aud(Some("another-client")), &key, ISSUER, AUDIENCE),
            Err(JwtError::InvalidAudience)
        ));
        // Multi-valued aud with azp == our client_id verifies.
        let verified: TestClaims =
            verify_rs256(&multi_aud(Some(AUDIENCE)), &key, ISSUER, AUDIENCE).expect("verifies");
        assert_eq!(verified.sub, "subject-1");
    }

    #[test]
    fn jwk_integers_decode_from_base64url_or_padded_standard_base64() {
        // Bytes that exercise both alphabet differences (0xfb -> '+'/'-', 0xff -> '/'/'_') and
        // need padding, with a leading zero like Battle.net's moduli.
        let bytes: &[u8] = &[0x00, 0xfb, 0xff, 0xbf, 0x10];
        let url = Base64UrlUnpadded::encode_string(bytes);
        let standard = base64ct::Base64::encode_string(bytes);
        assert!(standard.contains('+') || standard.contains('/'));
        assert!(standard.ends_with('='));
        // The leading zero is stripped: the minimal encoding of the same integer.
        assert_eq!(decode_jwk_integer(&url).unwrap(), &bytes[1..]);
        assert_eq!(decode_jwk_integer(&standard).unwrap(), &bytes[1..]);
        assert!(decode_jwk_integer("not base64!").is_err());
    }

    #[test]
    fn builds_a_key_from_battle_nets_published_format() {
        // Battle.net's JWKS publishes `n` as padded standard base64; this is one of its keys.
        let n = "AMyHr4r//CLrN25KyrGT31kQE4Q5zffJxEI1ZWOkNha1cqQkdrUTtxvu2cOZNI3TZ3sOQ3MDIxBIqNqVptdltO+qn+dfYp8b2hafkp31ywcDxCy14fZZxPumgaXeXUBRBA8akLAZRihyYupjSqxn2bjvaBkDL5krgPlJrhHs29tHQ1My6wZvOdoEslnffptv46b49dronMv01H6J67EGOH0ngMfQlWXZxE7DRqvGPAU/80tII3wrQkXO5u16GqjXd0zyGeMOEF7q2/CDtxwliX1hEE6YROQ07GjaiCLQ1HRDDxmvo16PV2Elj9u5pGXoj25fSzWUXUhRqwceNO7qrXs=";
        assert!(rsa_verifying_key(n, "AQAB").is_ok());
    }

    #[test]
    fn verifies_against_a_jwk_published_the_way_battle_net_does() {
        // Battle.net publishes `n` with a leading zero byte, in padded standard base64. Rebuild
        // the key that way and the signature must still verify.
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("generate key");
        let signing_key = SigningKey::<Sha256>::new(private_key.clone());
        let public_key = private_key.to_public_key();
        let mut n = vec![0u8];
        n.extend_from_slice(&public_key.n().to_be_bytes_trimmed_vartime());
        let key = rsa_verifying_key(
            &base64ct::Base64::encode_string(&n),
            &base64ct::Base64::encode_string(&public_key.e().to_be_bytes_trimmed_vartime()),
        )
        .expect("build verifying key from a Battle.net-style jwk");

        let future = (OffsetDateTime::now_utc() + Duration::hours(1)).unix_timestamp();
        let token =
            generate_signed_jwt(&signing_key, &header(), &claims(future, "subject-1")).unwrap();
        let verified: TestClaims = verify_rs256(&token, &key, ISSUER, AUDIENCE).expect("verifies");
        assert_eq!(verified.sub, "subject-1");
    }
}
