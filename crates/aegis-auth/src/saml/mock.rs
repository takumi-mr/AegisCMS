//! A mock SAML IdP for tests: reads the `AuthnRequest` from the redirect URL and issues signed
//! responses. The RSA key is generated once per test process (no private key in the repository).

use super::c14n::canonicalize;
use super::dsig::{ALG_ENVELOPED, ALG_EXC_C14N, NS_DSIG};
use super::xml::parse;
use super::{NS_ASSERTION, NS_PROTOCOL};
use crate::config::SamlConfig;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::OnceLock;

pub const IDP_ENTITY_ID: &str = "https://idp.example.test/saml";
pub const IDP_SSO_URL: &str = "https://idp.example.test/sso";
pub const SP_ENTITY_ID: &str = "https://cms.example.test/saml";
pub const ACS_URL: &str = "https://cms.example.test/api/v1/auth/saml/acs";

/// One key per process: RSA key generation takes a while
fn shared_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    KEY.get_or_init(|| RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("RSA key"))
}

/// A user signing in at the mock IdP
#[derive(Debug, Clone)]
pub struct MockUser {
    pub name_id: String,
    pub name_id_format: String,
    /// `(Name, values)`
    pub attributes: Vec<(String, Vec<String>)>,
}

impl MockUser {
    pub fn new(name_id: &str) -> Self {
        Self {
            name_id: name_id.to_string(),
            name_id_format: "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent".to_string(),
            attributes: Vec::new(),
        }
    }

    pub fn attribute(mut self, name: &str, values: &[&str]) -> Self {
        self.attributes.push((
            name.to_string(),
            values.iter().map(|v| v.to_string()).collect(),
        ));
        self
    }
}

pub struct MockSamlIdp {
    key: RsaPrivateKey,
}

impl Default for MockSamlIdp {
    fn default() -> Self {
        Self::new()
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn instant(offset_secs: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(offset_secs))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl MockSamlIdp {
    pub fn new() -> Self {
        Self {
            key: shared_key().clone(),
        }
    }

    /// An IdP with its own key (its responses do not verify with `config()` of another IdP)
    pub fn with_new_key() -> Self {
        Self {
            key: RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("RSA key"),
        }
    }

    pub fn public_key_pem(&self) -> String {
        self.key
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("PEM")
    }

    pub fn private_key(&self) -> &RsaPrivateKey {
        &self.key
    }

    /// `[auth.saml]` for an SP that trusts this IdP
    pub fn config(&self) -> SamlConfig {
        SamlConfig {
            enabled: true,
            display_name: "Mock SAML".to_string(),
            entity_id: SP_ENTITY_ID.to_string(),
            acs_url: ACS_URL.to_string(),
            idp_entity_id: IDP_ENTITY_ID.to_string(),
            idp_sso_url: IDP_SSO_URL.to_string(),
            idp_certificates: vec![self.public_key_pem()],
            ..Default::default()
        }
    }

    /// The `ID` of the `AuthnRequest` in the redirect to the IdP
    pub fn request_id(redirect_url: &str) -> String {
        let url = reqwest::Url::parse(redirect_url).expect("redirect URL");
        let encoded = url
            .query_pairs()
            .find(|(k, _)| k == "SAMLRequest")
            .expect("SAMLRequest")
            .1
            .into_owned();
        let deflated = STANDARD.decode(encoded).expect("base64");
        let mut xml = String::new();
        flate2::read::DeflateDecoder::new(deflated.as_slice())
            .read_to_string(&mut xml)
            .expect("DEFLATE");
        parse(&xml)
            .expect("AuthnRequest")
            .attr("ID")
            .expect("ID")
            .to_string()
    }

    /// An unsigned assertion answering `request_id`
    pub fn assertion(&self, request_id: &str, user: &MockUser) -> String {
        let attributes: String = user
            .attributes
            .iter()
            .map(|(name, values)| {
                let values: String = values
                    .iter()
                    .map(|v| {
                        format!(
                            r#"<saml:AttributeValue xsi:type="xs:string">{}</saml:AttributeValue>"#,
                            escape(v)
                        )
                    })
                    .collect();
                format!(
                    r#"<saml:Attribute Name="{}" NameFormat="urn:oasis:names:tc:SAML:2.0:attrname-format:basic">{}</saml:Attribute>"#,
                    escape(name),
                    values
                )
            })
            .collect();
        format!(
            concat!(
                r#"<saml:Assertion xmlns:saml="{ns}" xmlns:xs="http://www.w3.org/2001/XMLSchema" "#,
                r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" ID="_a{rid}" Version="2.0" IssueInstant="{now}">"#,
                r#"<saml:Issuer>{idp}</saml:Issuer>"#,
                r#"<saml:Subject><saml:NameID Format="{format}">{name_id}</saml:NameID>"#,
                r#"<saml:SubjectConfirmation Method="urn:oasis:names:tc:SAML:2.0:cm:bearer">"#,
                r#"<saml:SubjectConfirmationData InResponseTo="{rid}" NotOnOrAfter="{later}" Recipient="{acs}"/>"#,
                r#"</saml:SubjectConfirmation></saml:Subject>"#,
                r#"<saml:Conditions NotBefore="{before}" NotOnOrAfter="{later}">"#,
                r#"<saml:AudienceRestriction><saml:Audience>{sp}</saml:Audience></saml:AudienceRestriction></saml:Conditions>"#,
                r#"<saml:AuthnStatement AuthnInstant="{now}" SessionIndex="_s{rid}"><saml:AuthnContext>"#,
                r#"<saml:AuthnContextClassRef>urn:oasis:names:tc:SAML:2.0:ac:classes:PasswordProtectedTransport</saml:AuthnContextClassRef>"#,
                r#"</saml:AuthnContext></saml:AuthnStatement>"#,
                r#"<saml:AttributeStatement>{attributes}</saml:AttributeStatement></saml:Assertion>"#
            ),
            ns = NS_ASSERTION,
            rid = escape(request_id),
            now = instant(0),
            before = instant(-60),
            later = instant(300),
            idp = IDP_ENTITY_ID,
            sp = SP_ENTITY_ID,
            acs = ACS_URL,
            format = escape(&user.name_id_format),
            name_id = escape(&user.name_id),
            attributes = attributes,
        )
    }

    /// A response carrying `assertion` (unsigned unless `sign` is applied to it)
    pub fn response(&self, request_id: &str, assertion: &str) -> String {
        format!(
            concat!(
                r#"<samlp:Response xmlns:samlp="{p}" xmlns:saml="{a}" ID="_r{rid}" Version="2.0" "#,
                r#"IssueInstant="{now}" Destination="{acs}" InResponseTo="{rid}">"#,
                r#"<saml:Issuer>{idp}</saml:Issuer><samlp:Status>"#,
                r#"<samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status>"#,
                "{assertion}</samlp:Response>"
            ),
            p = NS_PROTOCOL,
            a = NS_ASSERTION,
            rid = escape(request_id),
            now = instant(0),
            acs = ACS_URL,
            idp = IDP_ENTITY_ID,
            assertion = assertion,
        )
    }

    /// Signs the root element of `xml` (which has an `ID`) with an enveloped signature placed
    /// after its `Issuer`, as the schema requires
    pub fn sign(&self, xml: &str) -> String {
        let element = parse(xml).expect("XML to sign");
        let id = element.attr("ID").expect("ID");
        let digest = STANDARD.encode(Sha256::digest(canonicalize(&element, None, &[]).as_bytes()));
        let signed_info = format!(
            concat!(
                r#"<ds:SignedInfo xmlns:ds="{ns}"><ds:CanonicalizationMethod Algorithm="{c14n}"/>"#,
                r#"<ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/>"#,
                r##"<ds:Reference URI="#{id}"><ds:Transforms><ds:Transform Algorithm="{env}"/>"##,
                r#"<ds:Transform Algorithm="{c14n}"/></ds:Transforms>"#,
                r#"<ds:DigestMethod Algorithm="http://www.w3.org/2001/04/xmlenc#sha256"/>"#,
                r#"<ds:DigestValue>{digest}</ds:DigestValue></ds:Reference></ds:SignedInfo>"#
            ),
            ns = NS_DSIG,
            c14n = ALG_EXC_C14N,
            env = ALG_ENVELOPED,
            id = id,
            digest = digest
        );
        let signed_info_c14n = canonicalize(&parse(&signed_info).expect("SignedInfo"), None, &[]);
        let value = rsa::pkcs1v15::SigningKey::<Sha256>::new(self.key.clone())
            .sign(signed_info_c14n.as_bytes())
            .to_vec();
        let signature = format!(
            r#"<ds:Signature xmlns:ds="{}">{}<ds:SignatureValue>{}</ds:SignatureValue></ds:Signature>"#,
            NS_DSIG,
            signed_info,
            STANDARD.encode(value)
        );
        let issuer_end = "</saml:Issuer>";
        let at = xml.find(issuer_end).expect("Issuer") + issuer_end.len();
        format!("{}{}{}", &xml[..at], signature, &xml[at..])
    }

    /// The form value of `SAMLResponse`
    pub fn encode(xml: &str) -> String {
        STANDARD.encode(xml)
    }

    /// A successful sign-in answering the request in `redirect_url`: a response with a signed
    /// assertion, encoded for the ACS form
    pub fn sign_in(&self, redirect_url: &str, user: &MockUser) -> String {
        let request_id = Self::request_id(redirect_url);
        let assertion = self.sign(&self.assertion(&request_id, user));
        Self::encode(&self.response(&request_id, &assertion))
    }
}
