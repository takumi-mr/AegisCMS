//! XML Signature verification for SAML messages: enveloped signatures, exclusive C14N, RSA with
//! SHA-256 / 384 / 512 (SHA-1 is refused), keys from the configured IdP certificates only.
//!
//! XML Signature Wrapping is prevented by not reading from the received document at all once the
//! signature is checked: `verify_enveloped` returns the signed element parsed again from exactly
//! the bytes that were digested. Whatever else the attacker adds to the document (a second
//! assertion, a moved signature, a duplicate `ID`) never reaches the caller.

use super::c14n::canonicalize;
use super::xml::{parse, Element};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use rsa::traits::PublicKeyParts;
use rsa::RsaPublicKey;
use sha2::{Digest, Sha256, Sha384, Sha512};
use x509_cert::der::{DecodePem, Encode};

pub const NS_DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
pub(crate) const ALG_EXC_C14N: &str = "http://www.w3.org/2001/10/xml-exc-c14n#";
pub(crate) const ALG_ENVELOPED: &str = "http://www.w3.org/2000/09/xmldsig#enveloped-signature";
/// RSA keys shorter than this are refused
const MIN_RSA_BITS: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hash {
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    fn from_digest_method(uri: &str) -> Option<Self> {
        match uri {
            "http://www.w3.org/2001/04/xmlenc#sha256" => Some(Self::Sha256),
            "http://www.w3.org/2001/04/xmldsig-more#sha384" => Some(Self::Sha384),
            "http://www.w3.org/2001/04/xmlenc#sha512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn from_signature_method(uri: &str) -> Option<Self> {
        match uri {
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256" => Some(Self::Sha256),
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha384" => Some(Self::Sha384),
            "http://www.w3.org/2001/04/xmldsig-more#rsa-sha512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn digest(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha256 => Sha256::digest(data).to_vec(),
            Self::Sha384 => Sha384::digest(data).to_vec(),
            Self::Sha512 => Sha512::digest(data).to_vec(),
        }
    }

    fn verify(self, key: &RsaPublicKey, message: &[u8], signature: &[u8]) -> bool {
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        let Ok(signature) = Signature::try_from(signature) else {
            return false;
        };
        match self {
            Self::Sha256 => VerifyingKey::<Sha256>::new(key.clone())
                .verify(message, &signature)
                .is_ok(),
            Self::Sha384 => VerifyingKey::<Sha384>::new(key.clone())
                .verify(message, &signature)
                .is_ok(),
            Self::Sha512 => VerifyingKey::<Sha512>::new(key.clone())
                .verify(message, &signature)
                .is_ok(),
        }
    }
}

/// The IdP's signing keys (`[auth.saml].idp_certificates`)
#[derive(Debug, Clone)]
pub struct TrustedKeys(Vec<RsaPublicKey>);

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    let compact: String = value.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    STANDARD
        .decode(compact)
        .map_err(|e| format!("invalid base64: {}", e))
}

impl TrustedKeys {
    /// Each entry is a PEM certificate, a PEM public key, or the base64 of a DER certificate
    /// (the `X509Certificate` of the IdP's metadata)
    pub fn parse(entries: &[String]) -> Result<Self, String> {
        let mut keys = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            let entry = entry.trim();
            let key = if entry.contains("-----BEGIN PUBLIC KEY-----") {
                RsaPublicKey::from_public_key_pem(entry).map_err(|e| e.to_string())
            } else if entry.contains("-----BEGIN CERTIFICATE-----") {
                x509_cert::Certificate::from_pem(entry)
                    .map_err(|e| e.to_string())
                    .and_then(|cert| rsa_key_of(&cert))
            } else {
                decode_base64(entry).and_then(|der| {
                    use x509_cert::der::Decode;
                    x509_cert::Certificate::from_der(&der)
                        .map_err(|e| e.to_string())
                        .and_then(|cert| rsa_key_of(&cert))
                })
            }
            .map_err(|e| format!("certificate #{}: {}", i + 1, e))?;
            if key.n().bits() < MIN_RSA_BITS {
                return Err(format!(
                    "certificate #{}: the RSA key is shorter than {} bits",
                    i + 1,
                    MIN_RSA_BITS
                ));
            }
            keys.push(key);
        }
        if keys.is_empty() {
            return Err("no certificate".to_string());
        }
        Ok(Self(keys))
    }
}

fn rsa_key_of(cert: &x509_cert::Certificate) -> Result<RsaPublicKey, String> {
    let spki = cert
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .map_err(|e| e.to_string())?;
    RsaPublicKey::from_public_key_der(&spki).map_err(|_| "not an RSA certificate".to_string())
}

fn dsig_child<'a>(parent: &'a Element, name: &str) -> Result<&'a Element, String> {
    parent
        .single_child(NS_DSIG, name)?
        .ok_or_else(|| format!("<{}> has no <{}>", parent.name, name))
}

fn algorithm(element: &Element) -> &str {
    element.attr("Algorithm").unwrap_or_default()
}

/// The `PrefixList` of an exclusive C14N `InclusiveNamespaces` child
fn inclusive_prefixes(method: &Element) -> Result<Vec<String>, String> {
    Ok(method
        .single_child(ALG_EXC_C14N, "InclusiveNamespaces")?
        .and_then(|list| list.attr("PrefixList"))
        .map(|list| list.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default())
}

/// Verifies the enveloped `signature` (a direct child of `signed`, the `ID`-carrying element of
/// `document` it must reference) and returns `signed` without the signature, parsed again from
/// its canonical form
pub fn verify_enveloped(
    document: &Element,
    signed: &Element,
    signature: &Element,
    keys: &TrustedKeys,
) -> Result<Element, String> {
    if !signed
        .child_elements()
        .any(|child| std::ptr::eq(child, signature))
    {
        return Err("the signature is not enveloped by the signed element".to_string());
    }
    let id = signed
        .attr("ID")
        .filter(|id| !id.is_empty())
        .ok_or("the signed element has no ID")?;
    // A second element with the same ID could be what another implementation resolves
    let same_id = document
        .descendants()
        .into_iter()
        .filter(|element| element.attr("ID") == Some(id))
        .count();
    if same_id != 1 {
        return Err(format!("ID '{}' is not unique", id));
    }

    let signed_info = dsig_child(signature, "SignedInfo")?;
    let c14n_method = dsig_child(signed_info, "CanonicalizationMethod")?;
    if algorithm(c14n_method) != ALG_EXC_C14N {
        return Err(format!(
            "unsupported canonicalization '{}'",
            algorithm(c14n_method)
        ));
    }
    let signature_method = dsig_child(signed_info, "SignatureMethod")?;
    let signature_hash =
        Hash::from_signature_method(algorithm(signature_method)).ok_or_else(|| {
            format!(
                "unsupported signature method '{}'",
                algorithm(signature_method)
            )
        })?;

    let reference = dsig_child(signed_info, "Reference")?;
    if reference.attr("URI") != Some(&format!("#{}", id)) {
        return Err(format!(
            "the signature references {:?}, not the signed element '#{}'",
            reference.attr("URI"),
            id
        ));
    }
    let mut enveloped = false;
    let mut prefixes = Vec::new();
    if let Some(transforms) = reference.single_child(NS_DSIG, "Transforms")? {
        for transform in transforms.child_elements() {
            match algorithm(transform) {
                ALG_ENVELOPED if transform.is(NS_DSIG, "Transform") && !enveloped => {
                    enveloped = true
                }
                ALG_EXC_C14N if transform.is(NS_DSIG, "Transform") => {
                    prefixes = inclusive_prefixes(transform)?
                }
                other => return Err(format!("unsupported transform '{}'", other)),
            }
        }
    }
    if !enveloped {
        return Err("the reference has no enveloped-signature transform".to_string());
    }
    let digest_method = dsig_child(reference, "DigestMethod")?;
    let digest_hash = Hash::from_digest_method(algorithm(digest_method))
        .ok_or_else(|| format!("unsupported digest method '{}'", algorithm(digest_method)))?;
    let expected_digest = decode_base64(&dsig_child(reference, "DigestValue")?.text())?;

    let canonical = canonicalize(signed, Some(signature), &prefixes);
    if digest_hash.digest(canonical.as_bytes()) != expected_digest {
        return Err("digest mismatch: the signed content was changed".to_string());
    }

    let signed_info_c14n = canonicalize(signed_info, None, &inclusive_prefixes(c14n_method)?);
    let signature_value = decode_base64(&dsig_child(signature, "SignatureValue")?.text())?;
    if !keys
        .0
        .iter()
        .any(|key| signature_hash.verify(key, signed_info_c14n.as_bytes(), &signature_value))
    {
        return Err("the signature does not match any IdP certificate".to_string());
    }

    let verified = parse(&canonical).map_err(|e| format!("canonical form: {}", e))?;
    if verified.name != signed.name || verified.namespace != signed.namespace {
        return Err("canonical form has a different root".to_string());
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saml::{NS_ASSERTION, NS_PROTOCOL};

    // Signed by signxml (libxml2's C14N), not by this module's canonicalization: see
    // tests/fixtures/saml/README.md
    const SIGNED_ASSERTION: &str = include_str!("../../tests/fixtures/saml/signed_assertion.xml");
    const SIGNED_RESPONSE: &str = include_str!("../../tests/fixtures/saml/signed_response.xml");
    const IDP_CERT: &str = include_str!("../../tests/fixtures/saml/idp_cert.pem");

    fn keys() -> TrustedKeys {
        TrustedKeys::parse(&[IDP_CERT.to_string()]).unwrap()
    }

    fn strip_declaration(xml: &str) -> &str {
        &xml[xml.find("?>").map_or(0, |i| i + 2)..]
    }

    /// Verifies the assertion's signature of a response document
    fn verify_assertion(xml: &str, keys: &TrustedKeys) -> Result<Element, String> {
        let document = parse(xml)?;
        let assertion = document
            .children_named(NS_ASSERTION, "Assertion")
            .next()
            .ok_or("no assertion")?;
        let signature = assertion
            .single_child(NS_DSIG, "Signature")?
            .ok_or("no signature")?;
        verify_enveloped(&document, assertion, signature, keys)
    }

    #[test]
    fn test_signatures_made_by_another_implementation_verify() {
        let assertion = verify_assertion(SIGNED_ASSERTION, &keys()).unwrap();
        // The verified copy has no signature, and a comment does not cut the NameID short
        assert!(assertion
            .single_child(NS_DSIG, "Signature")
            .unwrap()
            .is_none());
        let subject = assertion
            .single_child(NS_ASSERTION, "Subject")
            .unwrap()
            .unwrap();
        let name_id = subject
            .single_child(NS_ASSERTION, "NameID")
            .unwrap()
            .unwrap();
        assert_eq!(name_id.text(), "alice@example.com");
        let statement = assertion
            .single_child(NS_ASSERTION, "AttributeStatement")
            .unwrap()
            .unwrap();
        let value = &statement.descendants()[2];
        assert_eq!(value.text(), "Alice & <Bob> \"Q\"\r");

        // RSA-SHA512 over the whole response (no InclusiveNamespaces)
        let document = parse(SIGNED_RESPONSE).unwrap();
        let signature = document
            .single_child(NS_DSIG, "Signature")
            .unwrap()
            .unwrap();
        let response = verify_enveloped(&document, &document, signature, &keys()).unwrap();
        assert!(response.is(NS_PROTOCOL, "Response"));
        assert!(response
            .single_child(NS_DSIG, "Signature")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_certificate_forms() {
        // The base64 of the DER certificate, as in the IdP's metadata
        let base64: String = IDP_CERT
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        let keys = TrustedKeys::parse(&[base64]).unwrap();
        verify_assertion(SIGNED_ASSERTION, &keys).unwrap();

        assert!(TrustedKeys::parse(&[]).is_err());
        assert!(TrustedKeys::parse(&["not a certificate".to_string()]).is_err());
        let small = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 1024).unwrap();
        let small = rsa::pkcs8::EncodePublicKey::to_public_key_pem(
            &small.to_public_key(),
            rsa::pkcs8::LineEnding::LF,
        )
        .unwrap();
        assert!(TrustedKeys::parse(&[small]).unwrap_err().contains("2048"));
    }

    #[test]
    fn test_changed_content_and_other_keys_are_refused() {
        let tampered = SIGNED_ASSERTION.replace("alice<!-- comment -->", "mallory");
        assert!(verify_assertion(&tampered, &keys())
            .unwrap_err()
            .contains("digest mismatch"));
        // Whitespace is signed too
        let tampered = SIGNED_ASSERTION.replace("  spaced   text  ", " spaced text ");
        assert!(verify_assertion(&tampered, &keys()).is_err());
        // The IdP's certificate in KeyInfo is not trusted; only the configured ones are
        let other =
            TrustedKeys::parse(&[crate::saml::mock::MockSamlIdp::new().public_key_pem()]).unwrap();
        assert!(verify_assertion(SIGNED_ASSERTION, &other)
            .unwrap_err()
            .contains("does not match any IdP certificate"));
    }

    #[test]
    fn test_weak_or_unexpected_algorithms_are_refused() {
        for (from, to) in [
            (
                "xmldsig-more#rsa-sha256",
                "http://www.w3.org/2000/09/xmldsig#rsa-sha1\" x=\"",
            ),
            (
                "http://www.w3.org/2001/04/xmlenc#sha256",
                "http://www.w3.org/2000/09/xmldsig#sha1",
            ),
            (
                r#"<ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#">"#,
                r#"<ds:CanonicalizationMethod Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#WithComments">"#,
            ),
            (
                r#"<ds:Transform Algorithm="http://www.w3.org/2000/09/xmldsig#enveloped-signature"/>"#,
                "",
            ),
            (
                r#"<ds:Transform Algorithm="http://www.w3.org/2001/10/xml-exc-c14n#">"#,
                r#"<ds:Transform Algorithm="http://www.w3.org/TR/1999/REC-xpath-19991116">"#,
            ),
        ] {
            let changed = SIGNED_ASSERTION.replacen(from, to, 1);
            assert_ne!(changed, SIGNED_ASSERTION, "{}", from);
            assert!(verify_assertion(&changed, &keys()).is_err(), "{}", to);
        }
    }

    #[test]
    fn test_wrapping_with_a_duplicate_id_is_refused() {
        // A forged assertion with the signed assertion's ID placed before it: an implementation
        // that resolves the reference by ID would check the original and read the forgery
        let body = strip_declaration(SIGNED_ASSERTION);
        let start = body.find("<saml:Assertion").unwrap();
        let end = body.find("</saml:Assertion>").unwrap() + "</saml:Assertion>".len();
        let forged = body[start..end].replace("alice<!-- comment -->", "admin");
        let wrapped = format!("{}{}{}", &body[..start], forged, &body[start..]);
        let document = parse(&wrapped).unwrap();
        let original = document
            .children_named(NS_ASSERTION, "Assertion")
            .nth(1)
            .unwrap();
        let signature = original
            .single_child(NS_DSIG, "Signature")
            .unwrap()
            .unwrap();
        assert!(verify_enveloped(&document, original, signature, &keys())
            .unwrap_err()
            .contains("not unique"));
        // The signature checked against an element other than its parent
        let forged = document
            .children_named(NS_ASSERTION, "Assertion")
            .next()
            .unwrap();
        assert!(verify_enveloped(&document, forged, signature, &keys())
            .unwrap_err()
            .contains("not enveloped"));
    }
}
