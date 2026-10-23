//! Exclusive XML Canonicalization 1.0 without comments
//! (<http://www.w3.org/2001/10/xml-exc-c14n#>), the canonicalization SAML signatures use.
//!
//! A namespace declaration is output only where it is visibly used (by the element's or an
//! attribute's prefix, or listed in the `InclusiveNamespaces` prefix list) and not already output
//! with the same value by an output ancestor, so the result does not depend on the context of the
//! canonicalized subtree.

use super::xml::{Element, Node, NS_XML};
use std::collections::BTreeMap;

/// Canonicalizes `element`, leaving out `exclude` (the enveloped signature) and its subtree.
/// `inclusive_prefixes` is the `PrefixList` (`#default` for the default namespace)
pub fn canonicalize(
    element: &Element,
    exclude: Option<&Element>,
    inclusive_prefixes: &[String],
) -> String {
    let mut out = String::new();
    write_element(
        &mut out,
        element,
        exclude,
        inclusive_prefixes,
        &BTreeMap::new(),
    );
    out
}

fn write_element(
    out: &mut String,
    element: &Element,
    exclude: Option<&Element>,
    inclusive_prefixes: &[String],
    rendered: &BTreeMap<String, String>,
) {
    // Visibly used namespaces: prefix → URI ("" is the default namespace, "" URI is none)
    let mut used: BTreeMap<String, String> = BTreeMap::new();
    used.insert(
        element.prefix.clone().unwrap_or_default(),
        element.namespace.clone().unwrap_or_default(),
    );
    for attribute in &element.attributes {
        if let (Some(prefix), Some(namespace)) = (&attribute.prefix, &attribute.namespace) {
            used.insert(prefix.clone(), namespace.clone());
        }
    }
    for prefix in inclusive_prefixes {
        let prefix = if prefix == "#default" { "" } else { prefix };
        if let Some(namespace) = element.namespaces.get(prefix) {
            used.insert(prefix.to_string(), namespace.clone());
        }
    }

    let mut declarations = Vec::new();
    for (prefix, namespace) in used {
        if prefix == "xml" && namespace == NS_XML {
            continue;
        }
        let current = rendered.get(&prefix).map(String::as_str);
        let needed = if prefix.is_empty() {
            // `xmlns=""` only to undo a default namespace an output ancestor declared
            current.unwrap_or("") != namespace
        } else {
            current != Some(namespace.as_str())
        };
        if needed {
            declarations.push((prefix, namespace));
        }
    }

    let qname = qualified(element.prefix.as_deref(), &element.name);
    out.push('<');
    out.push_str(&qname);
    // Namespace declarations first, by prefix (the default namespace sorts first)
    for (prefix, namespace) in &declarations {
        if prefix.is_empty() {
            out.push_str(" xmlns=\"");
        } else {
            out.push_str(" xmlns:");
            out.push_str(prefix);
            out.push_str("=\"");
        }
        escape_attribute(out, namespace);
        out.push('"');
    }
    // Then attributes, by namespace URI (none first) and local name
    let mut attributes: Vec<_> = element.attributes.iter().collect();
    attributes.sort_by(|a, b| {
        (a.namespace.as_deref().unwrap_or(""), a.name.as_str())
            .cmp(&(b.namespace.as_deref().unwrap_or(""), b.name.as_str()))
    });
    for attribute in attributes {
        out.push(' ');
        out.push_str(&qualified(attribute.prefix.as_deref(), &attribute.name));
        out.push_str("=\"");
        escape_attribute(out, &attribute.value);
        out.push('"');
    }
    out.push('>');

    let mut child_rendered;
    let rendered = if declarations.is_empty() {
        rendered
    } else {
        child_rendered = rendered.clone();
        child_rendered.extend(declarations);
        &child_rendered
    };
    for child in &element.children {
        match child {
            Node::Text(text) => escape_text(out, text),
            Node::Element(child) => {
                if exclude.is_some_and(|excluded| std::ptr::eq(excluded, child)) {
                    continue;
                }
                write_element(out, child, exclude, inclusive_prefixes, rendered);
            }
        }
    }
    out.push_str("</");
    out.push_str(&qname);
    out.push('>');
}

fn qualified(prefix: Option<&str>, name: &str) -> String {
    match prefix {
        Some(prefix) => format!("{}:{}", prefix, name),
        None => name.to_string(),
    }
}

fn escape_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
}

fn escape_attribute(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::xml::parse;
    use super::*;

    fn c14n(xml: &str, prefixes: &[&str]) -> String {
        let prefixes: Vec<String> = prefixes.iter().map(|p| p.to_string()).collect();
        canonicalize(&parse(xml).unwrap(), None, &prefixes)
    }

    #[test]
    fn test_examples_from_the_exc_c14n_spec() {
        // Section 2.2: the unused n1 declaration is dropped, n2 is rendered where used
        let xml = r#"<n0:local xmlns:n0="foo:bar" xmlns:n3="ftp://example.org"><n1:elem2 xmlns:n1="http://example.net" xml:lang="en"><n3:stuff xmlns:n3="ftp://example.org"/></n1:elem2></n0:local>"#;
        let doc = parse(xml).unwrap();
        let elem2 = doc.child_elements().next().unwrap();
        assert_eq!(
            canonicalize(elem2, None, &[]),
            r#"<n1:elem2 xmlns:n1="http://example.net" xml:lang="en"><n3:stuff xmlns:n3="ftp://example.org"></n3:stuff></n1:elem2>"#
        );
        // With n3 in the prefix list it is rendered on the apex
        assert_eq!(
            canonicalize(elem2, None, &["n3".to_string()]),
            r#"<n1:elem2 xmlns:n1="http://example.net" xmlns:n3="ftp://example.org" xml:lang="en"><n3:stuff></n3:stuff></n1:elem2>"#
        );
    }

    #[test]
    fn test_ordering_escaping_and_default_namespace() {
        assert_eq!(
            c14n(
                "<a xmlns:z=\"urn:z\" xmlns:b=\"urn:b\" z:k=\"1\" b:k=\"2\" y=\"&lt;&quot;&#9;\" x='&apos;'>a&gt;b&#xD;<!--c-->c</a>",
                &[]
            ),
            "<a xmlns:b=\"urn:b\" xmlns:z=\"urn:z\" x=\"'\" y=\"&lt;&quot;&#x9;\" b:k=\"2\" z:k=\"1\">a&gt;b&#xD;c</a>"
        );
        assert_eq!(
            c14n(
                r#"<a xmlns="urn:a"><b xmlns=""><c xmlns="urn:a"/></b></a>"#,
                &[]
            ),
            r#"<a xmlns="urn:a"><b xmlns=""><c xmlns="urn:a"></c></b></a>"#
        );
        // An empty default namespace is never declared at the apex
        assert_eq!(c14n(r#"<a xmlns=""/>"#, &[]), "<a></a>");
    }
}
