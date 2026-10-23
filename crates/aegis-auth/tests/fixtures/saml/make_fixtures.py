"""Signs SAML fixtures with signxml (libxml2 exclusive C14N) for aegis-auth's tests."""
import datetime, sys
from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID
from lxml import etree
from signxml import XMLSigner, methods, SignatureMethod, DigestAlgorithm

out = sys.argv[1]
key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, "AegisCMS test IdP (fixture)")])
now = datetime.datetime(2026, 1, 1, tzinfo=datetime.timezone.utc)
cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
        .public_key(key.public_key()).serial_number(1)
        .not_valid_before(now).not_valid_after(now + datetime.timedelta(days=3650))
        .sign(key, hashes.SHA256()))
cert_pem = cert.public_bytes(serialization.Encoding.PEM)

# Pretty-printed, several prefixes, a default namespace, an unused declaration, xsi:type with a
# QName value, attributes out of order, escaped characters, a CR char ref and a comment
response = """<?xml version="1.0" encoding="UTF-8"?>
<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" xmlns:unused="urn:unused" Version="2.0" ID="_resp1" InResponseTo="_req1" IssueInstant="2026-01-01T00:00:00Z" Destination="https://cms.example.test/api/v1/auth/saml/acs">
  <saml:Issuer>https://idp.example.test/saml</saml:Issuer>
  <samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status>
  <saml:Assertion xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" Version="2.0" ID="_assert1" IssueInstant="2026-01-01T00:00:00Z">
    <saml:Issuer>https://idp.example.test/saml</saml:Issuer>
    <saml:Subject>
      <saml:NameID Format="urn:oasis:names:tc:SAML:2.0:nameid-format:persistent">alice<!-- comment -->@example.com</saml:NameID>
    </saml:Subject>
    <saml:AttributeStatement>
      <saml:Attribute Name="displayName" FriendlyName="cn"><saml:AttributeValue xsi:type="xs:string">Alice &amp; &lt;Bob&gt; &quot;Q&quot;&#13;</saml:AttributeValue></saml:Attribute>
      <saml:Attribute Name="note"><Value xmlns="urn:default" b="2" a="1&#10;x">  spaced   text  </Value></saml:Attribute>
    </saml:AttributeStatement>
  </saml:Assertion>
</samlp:Response>
"""

def sign(root, ref, method, digest, prefixes=None):
    signer = XMLSigner(method=methods.enveloped, signature_algorithm=method,
                       digest_algorithm=digest, c14n_algorithm="http://www.w3.org/2001/10/xml-exc-c14n#")
    signer.namespaces = {"ds": "http://www.w3.org/2000/09/xmldsig#"}
    if prefixes:
        signer.excise_empty_xmlns_declarations = False
    kwargs = {"key": key, "cert": cert_pem, "reference_uri": ref}
    if prefixes:
        kwargs["inclusive_ns_prefixes"] = prefixes
    return signer.sign(root, **kwargs)

parser = etree.XMLParser(remove_blank_text=False)
doc = etree.fromstring(response.encode(), parser)

# 1. The assertion signed (signature after its Issuer), the response not. Built by hand so the
#    PrefixList sits in the reference's exc-c14n transform as well (signxml only puts it under
#    CanonicalizationMethod); both canonicalizations are still libxml2's
import base64, hashlib
from cryptography.hazmat.primitives.asymmetric import padding
DS = "http://www.w3.org/2000/09/xmldsig#"
EXC = "http://www.w3.org/2001/10/xml-exc-c14n#"
assertion = doc.find("{urn:oasis:names:tc:SAML:2.0:assertion}Assertion")
digest = base64.b64encode(hashlib.sha256(etree.tostring(
    assertion, method="c14n", exclusive=True, with_comments=False, inclusive_ns_prefixes=["xs"])).digest()).decode()
signed_info_xml = f"""<ds:SignedInfo xmlns:ds="{DS}"><ds:CanonicalizationMethod Algorithm="{EXC}"><ec:InclusiveNamespaces xmlns:ec="{EXC}" PrefixList="xs"/></ds:CanonicalizationMethod><ds:SignatureMethod Algorithm="http://www.w3.org/2001/04/xmldsig-more#rsa-sha256"/><ds:Reference URI="#_assert1"><ds:Transforms><ds:Transform Algorithm="{DS}enveloped-signature"/><ds:Transform Algorithm="{EXC}"><ec:InclusiveNamespaces xmlns:ec="{EXC}" PrefixList="xs"/></ds:Transform></ds:Transforms><ds:DigestMethod Algorithm="http://www.w3.org/2001/04/xmlenc#sha256"/><ds:DigestValue>{digest}</ds:DigestValue></ds:Reference></ds:SignedInfo>"""
sig = etree.fromstring(f'<ds:Signature xmlns:ds="{DS}">{signed_info_xml}<ds:SignatureValue/></ds:Signature>')
issuer = assertion[0]
sig.tail = None  # the text around it is signed as it is
assertion.insert(1, sig)
signed_info = sig[0]
c14n_si = etree.tostring(signed_info, method="c14n", exclusive=True, with_comments=False, inclusive_ns_prefixes=["xs"])
sig[1].text = base64.b64encode(key.sign(c14n_si, padding.PKCS1v15(), hashes.SHA256())).decode()
open(f"{out}/signed_assertion.xml", "wb").write(etree.tostring(doc, xml_declaration=True, encoding="UTF-8"))

# 2. The whole response signed with RSA-SHA512
doc2 = etree.fromstring(response.encode(), parser)
signed_response = sign(doc2, "_resp1", SignatureMethod.RSA_SHA512, DigestAlgorithm.SHA512)
open(f"{out}/signed_response.xml", "wb").write(etree.tostring(signed_response, xml_declaration=True, encoding="UTF-8"))

open(f"{out}/idp_cert.pem", "wb").write(cert_pem)
print("ok")
