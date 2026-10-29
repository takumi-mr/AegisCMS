//! SAML 2.0 login as an `AuthProvider`: SP-initiated Web Browser SSO, the `AuthnRequest` over the
//! HTTP-Redirect binding and the `Response` over HTTP-POST.
//!
//! `initiate_login` stores the request ID in the cache (one-time, 10 minutes) and returns the
//! IdP's SSO URL; `authenticate` takes the posted `SAMLResponse`, verifies its signature with the
//! configured IdP certificates (see `dsig` for the XML Signature Wrapping defence), checks the
//! assertion (issuer, audience, validity period, bearer confirmation answering a pending request)
//! and maps the NameID and attributes to an AegisCMS account. Encrypted assertions and IdP-initiated
//! logins (responses that answer no request) are not supported.

mod c14n;
mod dsig;
#[cfg(any(test, feature = "mock-idp"))]
pub mod mock;
mod xml;

use aegis_core::{
    AegisError, AuthCredentials, AuthIdentity, AuthProvider, AuthRedirect, CacheBackend, Result,
};
use aegis_db::DatabaseAdapter;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use super::config::SamlConfig;
use super::sso::{auth_error, random_token, validate_roles, AccountPolicy, IdpProfile};
pub use super::sso::{
    REASON_ACCOUNT_DISABLED, REASON_IDP_ERROR, REASON_INVALID_STATE, REASON_INVALID_TOKEN,
    REASON_MISSING_USERNAME, REASON_NOT_PROVISIONED, REASON_USERNAME_TAKEN,
};
use super::{saml_subject, SAML_PROVIDER, SAML_RESPONSE_CREDENTIALS};
use dsig::{verify_enveloped, TrustedKeys, NS_DSIG};
use xml::Element;

pub const NS_PROTOCOL: &str = "urn:oasis:names:tc:SAML:2.0:protocol";
pub const NS_ASSERTION: &str = "urn:oasis:names:tc:SAML:2.0:assertion";
const NS_METADATA: &str = "urn:oasis:names:tc:SAML:2.0:metadata";
const STATUS_SUCCESS: &str = "urn:oasis:names:tc:SAML:2.0:status:Success";
const CM_BEARER: &str = "urn:oasis:names:tc:SAML:2.0:cm:bearer";
const BINDING_POST: &str = "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST";
const NAME_ID_TRANSIENT: &str = "urn:oasis:names:tc:SAML:2.0:nameid-format:transient";
const NAME_ID_EMAIL: &str = "urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress";

const REQUEST_KEY_PREFIX: &str = "saml:request:";
/// How long the user has to finish signing in at the IdP
const REQUEST_TTL_SECS: u64 = 600;
/// Clock skew tolerated for the validity periods
const LEEWAY_SECS: i64 = 60;
/// Larger responses are refused before parsing (real ones are a few KiB)
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

fn invalid(detail: impl std::fmt::Display) -> AegisError {
    tracing::warn!("Rejected SAML response: {}", detail);
    auth_error(REASON_INVALID_TOKEN)
}

/// Escapes text and attribute values of the XML this module writes
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn parse_instant(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| invalid(format!("invalid time '{}'", value)))
}

/// `NotBefore` / `NotOnOrAfter` of an element, with the leeway
fn check_period(element: &Element, now: DateTime<Utc>) -> Result<()> {
    let leeway = chrono::Duration::seconds(LEEWAY_SECS);
    if let Some(not_before) = element.attr("NotBefore") {
        if now + leeway < parse_instant(not_before)? {
            return Err(invalid(format!("<{}> is not yet valid", element.name)));
        }
    }
    if let Some(not_on_or_after) = element.attr("NotOnOrAfter") {
        if now - leeway >= parse_instant(not_on_or_after)? {
            return Err(invalid(format!("<{}> has expired", element.name)));
        }
    }
    Ok(())
}

fn single<'a>(parent: &'a Element, namespace: &str, name: &str) -> Result<Option<&'a Element>> {
    parent.single_child(namespace, name).map_err(invalid)
}

fn required<'a>(parent: &'a Element, namespace: &str, name: &str) -> Result<&'a Element> {
    single(parent, namespace, name)?
        .ok_or_else(|| invalid(format!("<{}> has no <{}>", parent.name, name)))
}

/// `AttributeStatement` values: `(Name, FriendlyName, values)`
struct Attributes(Vec<(String, Option<String>, Vec<String>)>);

impl Attributes {
    fn of(assertion: &Element) -> Self {
        let mut attributes = Vec::new();
        for statement in assertion.children_named(NS_ASSERTION, "AttributeStatement") {
            for attribute in statement.children_named(NS_ASSERTION, "Attribute") {
                let Some(name) = attribute.attr("Name") else {
                    continue;
                };
                let values = attribute
                    .children_named(NS_ASSERTION, "AttributeValue")
                    .map(|value| value.text().trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect();
                attributes.push((
                    name.to_string(),
                    attribute.attr("FriendlyName").map(str::to_string),
                    values,
                ));
            }
        }
        Self(attributes)
    }

    /// Values of the attribute whose `Name` (or else `FriendlyName`) is `name`
    fn values(&self, name: &str) -> Vec<String> {
        if name.is_empty() {
            return Vec::new();
        }
        let by_name: Vec<String> = self
            .0
            .iter()
            .filter(|(n, _, _)| n == name)
            .flat_map(|(_, _, values)| values.iter().cloned())
            .collect();
        if !by_name.is_empty() {
            return by_name;
        }
        self.0
            .iter()
            .filter(|(_, friendly, _)| friendly.as_deref() == Some(name))
            .flat_map(|(_, _, values)| values.iter().cloned())
            .collect()
    }

    fn first(&self, name: &str) -> Option<String> {
        self.values(name).into_iter().next()
    }

    fn to_json(&self) -> Value {
        let mut map = Map::new();
        for (name, _, values) in &self.0 {
            let entry = map
                .entry(name.clone())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(items) = entry {
                items.extend(values.iter().map(|v| json!(v)));
            }
        }
        Value::Object(map)
    }
}

/// What a verified assertion says about the user
struct VerifiedLogin {
    request_id: String,
    name_id: String,
    name_id_format: Option<String>,
    session_index: Option<String>,
    attributes: Attributes,
}

pub struct SamlAuthProvider {
    config: SamlConfig,
    keys: TrustedKeys,
    cache: Arc<dyn CacheBackend>,
    db: Arc<dyn DatabaseAdapter>,
}

impl SamlAuthProvider {
    /// Checks the settings and the IdP certificates
    pub fn new(
        config: SamlConfig,
        cache: Arc<dyn CacheBackend>,
        db: Arc<dyn DatabaseAdapter>,
    ) -> Result<Self> {
        for (key, value) in [
            ("entity_id", &config.entity_id),
            ("acs_url", &config.acs_url),
            ("idp_entity_id", &config.idp_entity_id),
            ("idp_sso_url", &config.idp_sso_url),
        ] {
            if value.trim().is_empty() {
                return Err(AegisError::Config(format!(
                    "[auth.saml].{} is required when SAML is enabled",
                    key
                )));
            }
        }
        for (key, value) in [
            ("acs_url", &config.acs_url),
            ("idp_sso_url", &config.idp_sso_url),
        ] {
            reqwest::Url::parse(value).map_err(|e| {
                AegisError::Config(format!("[auth.saml].{} is not a URL: {}", key, e))
            })?;
        }
        if config.name_id_format == NAME_ID_TRANSIENT {
            return Err(AegisError::Config(
                "[auth.saml].name_id_format: a transient NameID changes on every login and cannot \
                 identify a user; use persistent or emailAddress"
                    .to_string(),
            ));
        }
        validate_roles("auth.saml", &config.default_role, &config.role_mapping)?;
        let keys = TrustedKeys::parse(&config.idp_certificates)
            .map_err(|e| AegisError::Config(format!("[auth.saml].idp_certificates: {}", e)))?;
        Ok(Self {
            config,
            keys,
            cache,
            db,
        })
    }

    /// The SP metadata to register at the IdP (`GET /api/v1/auth/saml/metadata`)
    pub fn sp_metadata(&self) -> String {
        let name_id_format = if self.config.name_id_format.is_empty() {
            String::new()
        } else {
            format!(
                "<md:NameIDFormat>{}</md:NameIDFormat>",
                escape(&self.config.name_id_format)
            )
        };
        format!(
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                "\n",
                r#"<md:EntityDescriptor xmlns:md="{md}" entityID="{entity}">"#,
                r#"<md:SPSSODescriptor AuthnRequestsSigned="false" WantAssertionsSigned="true" "#,
                r#"protocolSupportEnumeration="{protocol}">{format}"#,
                r#"<md:AssertionConsumerService Binding="{post}" Location="{acs}" index="0" isDefault="true"/>"#,
                "</md:SPSSODescriptor></md:EntityDescriptor>\n"
            ),
            md = NS_METADATA,
            entity = escape(&self.config.entity_id),
            protocol = NS_PROTOCOL,
            format = name_id_format,
            post = BINDING_POST,
            acs = escape(&self.config.acs_url),
        )
    }

    fn account_policy(&self) -> AccountPolicy<'_> {
        AccountPolicy {
            provider: SAML_PROVIDER,
            auto_create_users: self.config.auto_create_users,
            default_role: &self.config.default_role,
            role_mapping: (!self.config.role_attribute.is_empty())
                .then_some(&self.config.role_mapping),
        }
    }

    /// The response and its single assertion, both taken only from signed content: the
    /// response's signature covers its assertion, otherwise the assertion must carry its own
    fn verified_parts(&self, document: &Element) -> Result<(Element, Element)> {
        let response = match single(document, NS_DSIG, "Signature")? {
            Some(signature) => {
                verify_enveloped(document, document, signature, &self.keys).map_err(invalid)?
            }
            None => document.clone(),
        };
        let response_signed = single(document, NS_DSIG, "Signature")?.is_some();
        if response
            .children_named(NS_ASSERTION, "EncryptedAssertion")
            .next()
            .is_some()
        {
            return Err(invalid("encrypted assertions are not supported"));
        }
        let assertion = required(&response, NS_ASSERTION, "Assertion")?;
        let assertion = match single(assertion, NS_DSIG, "Signature")? {
            Some(signature) => {
                verify_enveloped(&response, assertion, signature, &self.keys).map_err(invalid)?
            }
            None if response_signed => assertion.clone(),
            None => return Err(invalid("neither the response nor the assertion is signed")),
        };
        Ok((response, assertion))
    }

    /// Checks a verified response and assertion (SAML Profiles 4.1.4.2–3)
    fn check_assertion(&self, response: &Element, assertion: &Element) -> Result<VerifiedLogin> {
        let now = Utc::now();
        let idp = self.config.idp_entity_id.trim();
        if response.attr("Version") != Some("2.0") || assertion.attr("Version") != Some("2.0") {
            return Err(invalid("not a SAML 2.0 message"));
        }
        if let Some(destination) = response.attr("Destination") {
            if destination != self.config.acs_url {
                return Err(invalid(format!(
                    "Destination '{}' is not the ACS",
                    destination
                )));
            }
        }
        if let Some(issuer) = single(response, NS_ASSERTION, "Issuer")? {
            if issuer.text().trim() != idp {
                return Err(invalid(format!("response issuer '{}'", issuer.text())));
            }
        }
        let issuer = required(assertion, NS_ASSERTION, "Issuer")?.text();
        if issuer.trim() != idp {
            return Err(invalid(format!("assertion issuer '{}'", issuer)));
        }

        let conditions = required(assertion, NS_ASSERTION, "Conditions")?;
        check_period(conditions, now)?;
        let mut restrictions = conditions
            .children_named(NS_ASSERTION, "AudienceRestriction")
            .peekable();
        if restrictions.peek().is_none() {
            return Err(invalid("the assertion has no AudienceRestriction"));
        }
        for restriction in restrictions {
            if !restriction
                .children_named(NS_ASSERTION, "Audience")
                .any(|audience| audience.text().trim() == self.config.entity_id)
            {
                return Err(invalid("the assertion is not for this SP (Audience)"));
            }
        }

        let subject = required(assertion, NS_ASSERTION, "Subject")?;
        let name_id = single(subject, NS_ASSERTION, "NameID")?.ok_or_else(|| {
            invalid("the subject has no NameID (encrypted IDs are not supported)")
        })?;
        let name_id_format = name_id.attr("Format").map(str::to_string);
        if name_id_format.as_deref() == Some(NAME_ID_TRANSIENT) {
            return Err(invalid(
                "a transient NameID cannot identify a user; configure persistent or emailAddress at the IdP",
            ));
        }
        let name_id_value = name_id.text().trim().to_string();
        if name_id_value.is_empty() {
            return Err(invalid("empty NameID"));
        }

        // A bearer confirmation for this ACS, still valid, answering one of our requests
        let mut request_id = None;
        for confirmation in subject.children_named(NS_ASSERTION, "SubjectConfirmation") {
            if confirmation.attr("Method") != Some(CM_BEARER) {
                continue;
            }
            let Some(data) = single(confirmation, NS_ASSERTION, "SubjectConfirmationData")? else {
                continue;
            };
            if data.attr("Recipient") != Some(self.config.acs_url.as_str())
                || data.attr("NotOnOrAfter").is_none()
                || check_period(data, now).is_err()
            {
                continue;
            }
            if let Some(id) = data.attr("InResponseTo").filter(|id| !id.is_empty()) {
                request_id = Some(id.to_string());
                break;
            }
        }
        let request_id = request_id.ok_or_else(|| {
            invalid("no valid bearer SubjectConfirmation answering a request (IdP-initiated logins are not supported)")
        })?;
        if let Some(in_response_to) = response.attr("InResponseTo") {
            if in_response_to != request_id {
                return Err(invalid(
                    "InResponseTo of the response and the assertion differ",
                ));
            }
        }
        let authn = required(assertion, NS_ASSERTION, "AuthnStatement")?;

        Ok(VerifiedLogin {
            request_id,
            name_id: name_id_value,
            name_id_format,
            session_index: authn.attr("SessionIndex").map(str::to_string),
            attributes: Attributes::of(assertion),
        })
    }

    async fn complete_response(&self, encoded: &str) -> Result<AuthIdentity> {
        let encoded: String = encoded
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        if encoded.len() > MAX_RESPONSE_BYTES / 3 * 4 + 4 {
            return Err(invalid("the response is too large"));
        }
        let bytes = STANDARD.decode(encoded).map_err(invalid)?;
        let text = String::from_utf8(bytes).map_err(|_| invalid("the response is not UTF-8"))?;
        let document = xml::parse(&text).map_err(invalid)?;
        if !document.is(NS_PROTOCOL, "Response") {
            return Err(invalid("not a SAML Response"));
        }
        // An error status usually comes unsigned and without an assertion. Reading it from the
        // unverified document is safe: it can only make the login fail
        let status = required(&document, NS_PROTOCOL, "Status")?;
        let code = required(status, NS_PROTOCOL, "StatusCode")?;
        if code.attr("Value") != Some(STATUS_SUCCESS) {
            let detail = code
                .child_elements()
                .filter_map(|c| c.attr("Value"))
                .collect::<Vec<_>>()
                .join(", ");
            tracing::warn!(
                "SAML IdP returned status {:?} ({}) {}",
                code.attr("Value"),
                detail,
                single(status, NS_PROTOCOL, "StatusMessage")?
                    .map(|m| m.text())
                    .unwrap_or_default()
            );
            return Err(auth_error(REASON_IDP_ERROR));
        }

        let (response, assertion) = self.verified_parts(&document)?;
        let login = self.check_assertion(&response, &assertion)?;

        // One-time: a replayed response (or one answering another SP's request) finds no request
        let key = format!("{}{}", REQUEST_KEY_PREFIX, login.request_id);
        if self.cache.get(&key).await?.is_none() {
            return Err(auth_error(REASON_INVALID_STATE));
        }
        self.cache.delete(&key).await?;

        let attributes = &login.attributes;
        let email = attributes.first(&self.config.email_attribute).or_else(|| {
            (login.name_id_format.as_deref() == Some(NAME_ID_EMAIL)).then(|| login.name_id.clone())
        });
        let username = if self.config.username_attribute.is_empty() {
            Some(login.name_id.clone())
        } else {
            attributes.first(&self.config.username_attribute)
        };
        let profile = IdpProfile {
            subject: saml_subject(&self.config.idp_entity_id, &login.name_id),
            username,
            email,
            display_name: attributes.first(&self.config.display_name_attribute),
            role_values: attributes.values(&self.config.role_attribute),
        };
        let user = self
            .account_policy()
            .sign_in(self.db.as_ref(), profile)
            .await?;

        let mut identity = AuthIdentity::new(user.username, user.email.unwrap_or_default());
        identity.display_name = user.display_name;
        identity.role = user.role;
        identity.metadata = HashMap::from([
            ("issuer".to_string(), json!(self.config.idp_entity_id)),
            ("name_id".to_string(), json!(login.name_id)),
            ("name_id_format".to_string(), json!(login.name_id_format)),
            ("session_index".to_string(), json!(login.session_index)),
            ("attributes".to_string(), attributes.to_json()),
        ]);
        Ok(identity)
    }
}

#[async_trait]
impl AuthProvider for SamlAuthProvider {
    fn provider_id(&self) -> &str {
        SAML_PROVIDER
    }

    async fn initiate_login(&self, _callback_url: &str) -> Result<Option<AuthRedirect>> {
        // An xs:ID must not start with a digit
        let id = format!("_{}", random_token());
        let name_id_policy = if self.config.name_id_format.is_empty() {
            r#"<samlp:NameIDPolicy AllowCreate="true"/>"#.to_string()
        } else {
            format!(
                r#"<samlp:NameIDPolicy Format="{}" AllowCreate="true"/>"#,
                escape(&self.config.name_id_format)
            )
        };
        let request = format!(
            concat!(
                r#"<samlp:AuthnRequest xmlns:samlp="{p}" xmlns:saml="{a}" ID="{id}" Version="2.0" "#,
                r#"IssueInstant="{now}" Destination="{dest}" AssertionConsumerServiceURL="{acs}" "#,
                r#"ProtocolBinding="{post}"><saml:Issuer>{entity}</saml:Issuer>{policy}</samlp:AuthnRequest>"#
            ),
            p = NS_PROTOCOL,
            a = NS_ASSERTION,
            id = id,
            now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            dest = escape(&self.config.idp_sso_url),
            acs = escape(&self.config.acs_url),
            post = BINDING_POST,
            entity = escape(&self.config.entity_id),
            policy = name_id_policy,
        );
        // HTTP-Redirect binding: raw DEFLATE, base64, then URL encoding
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(request.as_bytes()).map_err(|e| {
            AegisError::Internal(format!("Failed to encode the AuthnRequest: {}", e))
        })?;
        let deflated = encoder.finish().map_err(|e| {
            AegisError::Internal(format!("Failed to encode the AuthnRequest: {}", e))
        })?;
        let mut url = reqwest::Url::parse(&self.config.idp_sso_url)
            .map_err(|e| AegisError::Config(format!("Invalid [auth.saml].idp_sso_url: {}", e)))?;
        url.query_pairs_mut()
            .append_pair("SAMLRequest", &STANDARD.encode(deflated));

        self.cache
            .set(
                &format!("{}{}", REQUEST_KEY_PREFIX, id),
                "1",
                Some(REQUEST_TTL_SECS),
            )
            .await?;
        Ok(Some(AuthRedirect {
            url: url.to_string(),
            method: "GET".to_string(),
            form_fields: HashMap::new(),
        }))
    }

    async fn authenticate(&self, credentials: &AuthCredentials) -> Result<AuthIdentity> {
        if credentials.kind != SAML_RESPONSE_CREDENTIALS {
            return Err(auth_error(super::local::REASON_UNSUPPORTED_CREDENTIALS));
        }
        let encoded = credentials
            .parameters
            .get("SAMLResponse")
            .filter(|r| !r.is_empty())
            .ok_or_else(|| invalid("no SAMLResponse"))?;
        self.complete_response(encoded).await
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{MockSamlIdp, MockUser, ACS_URL, IDP_ENTITY_ID, SP_ENTITY_ID};
    use super::*;
    use crate::users::{create_user, NewUser};
    use aegis_cache::MemoryCacheBackend;
    use aegis_db::SqliteAdapter;

    struct Setup {
        idp: MockSamlIdp,
        db: Arc<dyn DatabaseAdapter>,
        provider: SamlAuthProvider,
    }

    async fn setup(configure: impl FnOnce(&mut SamlConfig)) -> Setup {
        let idp = MockSamlIdp::new();
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        adapter.init().await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        let mut config = idp.config();
        config.role_attribute = "groups".to_string();
        config.role_mapping = HashMap::from([
            ("cms-editors".to_string(), "editor".to_string()),
            ("cms-admins".to_string(), "admin".to_string()),
        ]);
        configure(&mut config);
        let provider =
            SamlAuthProvider::new(config, Arc::new(MemoryCacheBackend::new()), db.clone()).unwrap();
        Setup { idp, db, provider }
    }

    impl Setup {
        /// The `ID` of a new pending `AuthnRequest`
        async fn request(&self) -> String {
            let url = self.provider.initiate_login("").await.unwrap().unwrap().url;
            MockSamlIdp::request_id(&url)
        }

        async fn post(&self, saml_response: String) -> Result<AuthIdentity> {
            self.provider
                .authenticate(&AuthCredentials {
                    kind: SAML_RESPONSE_CREDENTIALS.to_string(),
                    parameters: HashMap::from([("SAMLResponse".to_string(), saml_response)]),
                    ..Default::default()
                })
                .await
        }

        async fn login(&self, user: &MockUser) -> Result<AuthIdentity> {
            let url = self.provider.initiate_login("").await.unwrap().unwrap().url;
            self.post(self.idp.sign_in(&url, user)).await
        }

        /// A response for a new request with the assertion changed by `edit` before signing
        async fn login_with(&self, edit: impl FnOnce(String) -> String) -> Result<AuthIdentity> {
            let id = self.request().await;
            let assertion = self.idp.sign(&edit(self.idp.assertion(&id, &alice(&[]))));
            self.post(MockSamlIdp::encode(&self.idp.response(&id, &assertion)))
                .await
        }
    }

    fn reason(result: Result<AuthIdentity>) -> String {
        match result {
            Err(AegisError::Authentication(reason)) => reason,
            other => panic!("expected an authentication error, got {:?}", other),
        }
    }

    fn alice(groups: &[&str]) -> MockUser {
        MockUser::new("u-123")
            .attribute("uid", &["alice"])
            .attribute("email", &["alice@example.com"])
            .attribute("displayName", &["Alice Liddell"])
            .attribute("groups", groups)
    }

    /// Replaces the value of the first `attribute="..."` in `xml`
    fn set_attr(xml: &str, attribute: &str, value: &str) -> String {
        let key = format!("{}=\"", attribute);
        let at = xml.find(&key).unwrap() + key.len();
        let end = at + xml[at..].find('"').unwrap();
        format!("{}{}{}", &xml[..at], value, &xml[end..])
    }

    /// Removes the first `<tag ...>...</tag>` from `xml`
    fn remove_element(xml: &str, tag: &str) -> String {
        let start = xml.find(&format!("<{}", tag)).unwrap();
        let close = format!("</{}>", tag);
        let end = xml.find(&close).unwrap() + close.len();
        format!("{}{}", &xml[..start], &xml[end..])
    }

    fn instant(offset_secs: i64) -> String {
        (Utc::now() + chrono::Duration::seconds(offset_secs))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    #[tokio::test]
    async fn test_authn_request_and_sp_metadata() {
        let setup = setup(|config| config.name_id_format = NAME_ID_EMAIL.to_string()).await;
        let url = setup
            .provider
            .initiate_login("")
            .await
            .unwrap()
            .unwrap()
            .url;
        assert!(url.starts_with("https://idp.example.test/sso?SAMLRequest="));
        let encoded = reqwest::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .next()
            .unwrap()
            .1
            .into_owned();
        let mut request = String::new();
        std::io::Read::read_to_string(
            &mut flate2::read::DeflateDecoder::new(STANDARD.decode(encoded).unwrap().as_slice()),
            &mut request,
        )
        .unwrap();
        let request = xml::parse(&request).unwrap();
        assert!(request.is(NS_PROTOCOL, "AuthnRequest"));
        assert!(request.attr("ID").unwrap().starts_with('_'));
        assert_eq!(request.attr("AssertionConsumerServiceURL"), Some(ACS_URL));
        assert_eq!(request.attr("ProtocolBinding"), Some(BINDING_POST));
        assert_eq!(
            required(&request, NS_ASSERTION, "Issuer").unwrap().text(),
            SP_ENTITY_ID
        );
        let policy = required(&request, NS_PROTOCOL, "NameIDPolicy").unwrap();
        assert_eq!(policy.attr("Format"), Some(NAME_ID_EMAIL));

        let metadata = setup.provider.sp_metadata();
        let parsed = xml::parse(&metadata).unwrap();
        assert_eq!(parsed.attr("entityID"), Some(SP_ENTITY_ID));
        assert!(metadata.contains(&format!(r#"Location="{}""#, ACS_URL)));
        assert!(metadata.contains(NAME_ID_EMAIL));
    }

    #[tokio::test]
    async fn test_first_login_creates_and_links_the_user() {
        let setup = setup(|config| config.username_attribute = "uid".to_string()).await;
        let identity = setup.login(&alice(&["cms-editors"])).await.unwrap();
        assert_eq!(identity.external_id, "alice");
        assert_eq!(identity.role, "editor");
        assert_eq!(identity.email, "alice@example.com");
        assert_eq!(identity.display_name.as_deref(), Some("Alice Liddell"));
        assert_eq!(identity.metadata["name_id"], json!("u-123"));
        assert_eq!(identity.metadata["issuer"], json!(IDP_ENTITY_ID));
        assert_eq!(
            identity.metadata["attributes"]["groups"],
            json!(["cms-editors"])
        );
        let subject = saml_subject(IDP_ENTITY_ID, "u-123");
        assert_eq!(
            setup
                .db
                .get_identity_link(SAML_PROVIDER, &subject)
                .await
                .unwrap()
                .as_deref(),
            Some("alice")
        );

        // The next login finds the same account (even if uid changes); the groups decide the role
        let mut renamed = alice(&["cms-admins", "cms-editors"]);
        renamed.attributes[0].1 = vec!["alice2".to_string()];
        let again = setup.login(&renamed).await.unwrap();
        assert_eq!(again.external_id, "alice");
        assert_eq!(again.role, "admin");
        assert_eq!(setup.login(&alice(&[])).await.unwrap().role, "viewer");
    }

    #[tokio::test]
    async fn test_name_id_is_the_default_user_name_and_email() {
        let setup = setup(|config| config.role_attribute = String::new()).await;
        let mut user = MockUser::new("bob@example.com");
        user.name_id_format = NAME_ID_EMAIL.to_string();
        let identity = setup.login(&user).await.unwrap();
        assert_eq!(identity.external_id, "bob@example.com");
        assert_eq!(identity.email, "bob@example.com");
        // Without role_attribute the account's role applies
        assert_eq!(identity.role, "viewer");
        crate::users::set_role(setup.db.as_ref(), "bob@example.com", "editor")
            .await
            .unwrap();
        assert_eq!(setup.login(&user).await.unwrap().role, "editor");
    }

    #[tokio::test]
    async fn test_provisioning_rules() {
        let local_user = |username: &str| NewUser {
            username: username.to_string(),
            password: "local-password".to_string(),
            role: "editor".to_string(),
            email: None,
            display_name: None,
        };
        let setup = setup(|config| config.auto_create_users = false).await;
        assert_eq!(
            reason(setup.login(&alice(&[])).await),
            REASON_NOT_PROVISIONED
        );
        create_user(setup.db.as_ref(), local_user("alice"))
            .await
            .unwrap();
        crate::users::link_identity(
            setup.db.as_ref(),
            "alice",
            SAML_PROVIDER,
            &saml_subject(IDP_ENTITY_ID, "u-123"),
        )
        .await
        .unwrap();
        let identity = setup.login(&alice(&["cms-editors"])).await.unwrap();
        assert_eq!(identity.external_id, "alice");

        // A local account with the same name is never taken over
        let setup = self::setup(|_| {}).await;
        create_user(setup.db.as_ref(), local_user("u-123"))
            .await
            .unwrap();
        assert_eq!(
            reason(setup.login(&alice(&[])).await),
            REASON_USERNAME_TAKEN
        );

        let setup = self::setup(|config| config.username_attribute = "uid".to_string()).await;
        assert_eq!(
            reason(setup.login(&MockUser::new("u-9")).await),
            REASON_MISSING_USERNAME
        );
        setup.login(&alice(&[])).await.unwrap();
        crate::users::set_disabled(setup.db.as_ref(), "alice", true)
            .await
            .unwrap();
        assert_eq!(
            reason(setup.login(&alice(&[])).await),
            REASON_ACCOUNT_DISABLED
        );
    }

    #[tokio::test]
    async fn test_responses_are_single_use_and_must_answer_a_request() {
        let setup = setup(|_| {}).await;
        let url = setup
            .provider
            .initiate_login("")
            .await
            .unwrap()
            .unwrap()
            .url;
        let response = setup.idp.sign_in(&url, &alice(&[]));
        setup.post(response.clone()).await.unwrap();
        assert_eq!(reason(setup.post(response).await), REASON_INVALID_STATE);

        // IdP-initiated: no InResponseTo
        let unsolicited = setup.idp.sign(
            &setup
                .idp
                .assertion("x", &alice(&[]))
                .replace(r#" InResponseTo="x""#, ""),
        );
        let response = setup
            .idp
            .response("x", &unsolicited)
            .replace(r#" InResponseTo="x""#, "");
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_TOKEN
        );
        // A request this SP never made (or another SP's)
        let unknown = setup.idp.sign(&setup.idp.assertion("_other", &alice(&[])));
        let response = setup.idp.response("_other", &unknown);
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_STATE
        );
    }

    #[tokio::test]
    async fn test_signature_placement() {
        let setup = setup(|_| {}).await;
        // The response signed, the assertion not: accepted
        let id = setup.request().await;
        let response = setup.idp.sign(
            &setup
                .idp
                .response(&id, &setup.idp.assertion(&id, &alice(&[]))),
        );
        setup.post(MockSamlIdp::encode(&response)).await.unwrap();

        // Neither signed
        let id = setup.request().await;
        let response = setup
            .idp
            .response(&id, &setup.idp.assertion(&id, &alice(&[])));
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_TOKEN
        );

        // Signed by another IdP's key
        let other = MockSamlIdp::with_new_key();
        let id = setup.request().await;
        let assertion = other.sign(&other.assertion(&id, &alice(&[])));
        let response = other.response(&id, &assertion);
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_TOKEN
        );

        // Changed after signing
        let id = setup.request().await;
        let assertion = setup
            .idp
            .sign(&setup.idp.assertion(&id, &alice(&["cms-editors"])))
            .replace("cms-editors", "cms-admins");
        let response = setup.idp.response(&id, &assertion);
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_TOKEN
        );
    }

    /// XML Signature Wrapping: a forged assertion next to (or around) a genuinely signed one
    #[tokio::test]
    async fn test_signature_wrapping_is_refused() {
        let setup = setup(|_| {}).await;
        let id = setup.request().await;
        let genuine = setup.idp.sign(&setup.idp.assertion(&id, &alice(&[])));
        let root = MockUser::new("root").attribute("groups", &["cms-admins"]);
        // The forgery has its own ID unless stated otherwise
        let forged = setup.idp.assertion(&id, &root).replacen(
            &format!(r#"ID="_a{}""#, id),
            r#"ID="_forged""#,
            1,
        );
        let signature_start = genuine.find("<ds:Signature").unwrap();
        let signature_end = genuine.find("</ds:Signature>").unwrap() + "</ds:Signature>".len();
        let signature = &genuine[signature_start..signature_end];
        let with_signature = |assertion: &str| {
            let at = assertion.find("</saml:Issuer>").unwrap() + "</saml:Issuer>".len();
            format!("{}{}{}", &assertion[..at], signature, &assertion[at..])
        };
        let with_genuine_id = forged.replacen(r#"ID="_forged""#, &format!(r#"ID="_a{}""#, id), 1);

        let attacks = [
            // A second assertion beside the signed one (either order)
            format!("{}{}", forged, genuine),
            format!("{}{}", genuine, forged),
            // The signed assertion hidden in Extensions, the forgery in its place
            format!("<samlp:Extensions>{}</samlp:Extensions>{}", genuine, forged),
            // The forgery carrying the genuine signature (which references another ID)
            with_signature(&forged),
            // ...with the genuine ID, the original hidden away (duplicate ID)
            format!(
                "<samlp:Extensions>{}</samlp:Extensions>{}",
                genuine,
                with_signature(&with_genuine_id)
            ),
            // ...with the genuine ID alone (digest mismatch)
            with_signature(&with_genuine_id),
        ];
        for (i, assertions) in attacks.iter().enumerate() {
            let response = setup.idp.response(&id, assertions);
            assert_eq!(
                reason(setup.post(MockSamlIdp::encode(&response)).await),
                REASON_INVALID_TOKEN,
                "attack #{}",
                i
            );
        }
        // The genuine response still works afterwards (no attack consumed the request)
        let response = setup.idp.response(&id, &genuine);
        let identity = setup.post(MockSamlIdp::encode(&response)).await.unwrap();
        assert_eq!(identity.external_id, "u-123");
        assert!(setup
            .db
            .get_user_by_username("root")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_comment_does_not_truncate_the_name_id() {
        // The signature covers the text without the comment: the whole text is the NameID
        let setup = setup(|_| {}).await;
        let identity = setup
            .login_with(|assertion| assertion.replace(">u-123<", ">admin<!---->.evil.example<"))
            .await
            .unwrap();
        assert_eq!(identity.external_id, "admin.evil.example");
    }

    #[tokio::test]
    async fn test_assertion_conditions_are_checked() {
        let setup = setup(|_| {}).await;
        type Tamper = Box<dyn Fn(String) -> String>;
        let cases: Vec<(&str, Tamper)> = vec![
            (
                "other audience",
                Box::new(|a| a.replace(SP_ENTITY_ID, "https://other-sp.example")),
            ),
            (
                "no audience restriction",
                Box::new(|a| remove_element(&a, "saml:AudienceRestriction")),
            ),
            (
                "other recipient",
                Box::new(|a| a.replace(ACS_URL, "https://other-sp.example/acs")),
            ),
            (
                "other issuer",
                Box::new(|a| a.replace(IDP_ENTITY_ID, "https://evil.example")),
            ),
            (
                "confirmation expired",
                Box::new(|a| set_attr(&a, "NotOnOrAfter", &instant(-600))),
            ),
            (
                "conditions expired",
                Box::new(|a| {
                    let at = a.find("<saml:Conditions").unwrap();
                    format!(
                        "{}{}",
                        &a[..at],
                        set_attr(&a[at..], "NotOnOrAfter", &instant(-600))
                    )
                }),
            ),
            (
                "not yet valid",
                Box::new(|a| set_attr(&a, "NotBefore", &instant(600))),
            ),
            (
                "holder-of-key confirmation",
                Box::new(|a| a.replace("cm:bearer", "cm:holder-of-key")),
            ),
            (
                "transient NameID",
                Box::new(|a| a.replace("nameid-format:persistent", "nameid-format:transient")),
            ),
            (
                "no AuthnStatement",
                Box::new(|a| remove_element(&a, "saml:AuthnStatement")),
            ),
        ];
        for (name, edit) in cases {
            assert_eq!(
                reason(setup.login_with(edit).await),
                REASON_INVALID_TOKEN,
                "{}",
                name
            );
        }

        // The response's Destination must be the ACS
        let id = setup.request().await;
        let assertion = setup.idp.sign(&setup.idp.assertion(&id, &alice(&[])));
        let response = set_attr(
            &setup.idp.response(&id, &assertion),
            "Destination",
            "https://other-sp.example/acs",
        );
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&response)).await),
            REASON_INVALID_TOKEN
        );
        assert!(setup.db.list_users().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_error_status_and_malformed_input() {
        let setup = setup(|_| {}).await;
        let id = setup.request().await;
        let failed = setup.idp.response(&id, "").replace(
            r#"status:Success"/>"#,
            r#"status:Responder"><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:AuthnFailed"/></samlp:StatusCode>"#,
        );
        assert_eq!(
            reason(setup.post(MockSamlIdp::encode(&failed)).await),
            REASON_IDP_ERROR
        );

        let dtd = format!(
            r#"<!DOCTYPE r [<!ENTITY e "x">]>{}"#,
            setup.idp.response(&id, "")
        );
        for input in [
            "not base64!".to_string(),
            MockSamlIdp::encode("<html/>"),
            MockSamlIdp::encode(&dtd),
            STANDARD.encode([0xff, 0xfe]),
            "A".repeat(MAX_RESPONSE_BYTES * 2),
        ] {
            assert_eq!(reason(setup.post(input).await), REASON_INVALID_TOKEN);
        }
        let other_kind = setup
            .provider
            .authenticate(&AuthCredentials {
                kind: "password".to_string(),
                ..Default::default()
            })
            .await;
        assert!(other_kind.is_err());
    }

    #[tokio::test]
    async fn test_config_is_validated() {
        let adapter = SqliteAdapter::connect("sqlite::memory:", 1).await.unwrap();
        let db: Arc<dyn DatabaseAdapter> = Arc::new(adapter);
        let cache: Arc<dyn CacheBackend> = Arc::new(MemoryCacheBackend::new());
        let idp = MockSamlIdp::new();
        let new = |config: SamlConfig| SamlAuthProvider::new(config, cache.clone(), db.clone());
        assert!(new(idp.config()).is_ok());
        let broken: [fn(&mut SamlConfig); 6] = [
            |c| c.entity_id = String::new(),
            |c| c.acs_url = "not a url".to_string(),
            |c| c.idp_certificates = Vec::new(),
            |c| c.idp_certificates = vec!["garbage".to_string()],
            |c| c.name_id_format = NAME_ID_TRANSIENT.to_string(),
            |c| c.role_mapping = HashMap::from([("g".to_string(), "owner".to_string())]),
        ];
        for (i, edit) in broken.iter().enumerate() {
            let mut config = idp.config();
            edit(&mut config);
            assert!(new(config).is_err(), "case #{}", i);
        }
    }
}
