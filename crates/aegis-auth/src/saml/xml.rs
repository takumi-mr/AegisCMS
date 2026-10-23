//! A small namespace-aware XML tree for SAML messages.
//!
//! Unlike general-purpose DOMs it keeps the prefixes as written (exclusive canonicalization
//! renders them) and refuses everything a SAML message does not need and an attacker could use:
//! DTDs (and so entity expansion), processing instructions, undeclared prefixes, duplicate
//! attributes and deep nesting. Comments are dropped, as exclusive C14N without comments does.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use std::collections::BTreeMap;

pub const NS_XML: &str = "http://www.w3.org/XML/1998/namespace";
/// Nesting limit (SAML responses are about 10 levels deep)
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub prefix: Option<String>,
    pub name: String,
    pub namespace: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// The prefix as written (`saml` in `<saml:Assertion>`)
    pub prefix: Option<String>,
    pub name: String,
    pub namespace: Option<String>,
    /// Attributes other than namespace declarations, in document order
    pub attributes: Vec<Attribute>,
    /// In-scope namespace bindings (`""` is the default namespace)
    pub namespaces: BTreeMap<String, String>,
    pub children: Vec<Node>,
}

impl Element {
    pub fn is(&self, namespace: &str, name: &str) -> bool {
        self.name == name && self.namespace.as_deref() == Some(namespace)
    }

    pub fn child_elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|node| match node {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    pub fn children_named<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> impl Iterator<Item = &'a Element> + 'a {
        self.child_elements()
            .filter(move |child| child.is(namespace, name))
    }

    /// The only child with this name; `Err` when there are several
    pub fn single_child(&self, namespace: &str, name: &str) -> Result<Option<&Element>, String> {
        let mut found = None;
        for child in self.child_elements() {
            if child.is(namespace, name) {
                if found.is_some() {
                    return Err(format!("<{}> has more than one <{}>", self.name, name));
                }
                found = Some(child);
            }
        }
        Ok(found)
    }

    /// An attribute without a namespace
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == name && a.namespace.is_none())
            .map(|a| a.value.as_str())
    }

    /// All text directly inside the element (text split by comments is joined, so a comment
    /// cannot cut off part of a value that was signed)
    pub fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|node| match node {
                Node::Text(text) => Some(text.as_str()),
                Node::Element(_) => None,
            })
            .collect()
    }

    /// The element and all its descendant elements, in document order
    pub fn descendants(&self) -> Vec<&Element> {
        let mut all = Vec::new();
        let mut pending = vec![self];
        while let Some(element) = pending.pop() {
            all.push(element);
            let children: Vec<&Element> = element.child_elements().collect();
            pending.extend(children.into_iter().rev());
        }
        all
    }
}

fn split_qname(qname: &str) -> Result<(Option<&str>, &str), String> {
    match qname.split_once(':') {
        Some((prefix, local))
            if !prefix.is_empty() && !local.is_empty() && !local.contains(':') =>
        {
            Ok((Some(prefix), local))
        }
        None if !qname.is_empty() => Ok((None, qname)),
        _ => Err(format!("invalid name '{}'", qname)),
    }
}

fn resolve_prefix(namespaces: &BTreeMap<String, String>, prefix: &str) -> Result<String, String> {
    if prefix == "xml" {
        return Ok(NS_XML.to_string());
    }
    namespaces
        .get(prefix)
        .cloned()
        .ok_or_else(|| format!("undeclared namespace prefix '{}'", prefix))
}

fn start_element(
    start: &BytesStart,
    parent_namespaces: Option<&BTreeMap<String, String>>,
) -> Result<Element, String> {
    let qname = start.name().as_ref().to_string();
    let mut namespaces = parent_namespaces.cloned().unwrap_or_default();
    let mut raw_attributes = Vec::new();
    let mut attributes = start.attributes();
    attributes.with_checks(true);
    for attribute in attributes {
        let attribute = attribute.map_err(|e| format!("invalid attribute: {}", e))?;
        let key = attribute.key.as_ref().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| format!("invalid value of attribute '{}': {}", key, e))?
            .into_owned();
        if key == "xmlns" {
            if value.is_empty() {
                namespaces.remove("");
            } else {
                namespaces.insert(String::new(), value);
            }
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            // Undeclaring a prefix (`xmlns:p=""`) is XML 1.1 only
            if value.is_empty() || prefix.is_empty() || prefix == "xml" || prefix == "xmlns" {
                return Err(format!("invalid namespace declaration '{}'", key));
            }
            namespaces.insert(prefix.to_string(), value);
        } else {
            raw_attributes.push((key, value));
        }
    }

    let (prefix, name) = split_qname(&qname)?;
    let namespace = match prefix {
        Some(prefix) => Some(resolve_prefix(&namespaces, prefix)?),
        None => namespaces.get("").cloned(),
    };
    let mut element = Element {
        prefix: prefix.map(str::to_string),
        name: name.to_string(),
        namespace,
        attributes: Vec::with_capacity(raw_attributes.len()),
        namespaces,
        children: Vec::new(),
    };
    for (key, value) in raw_attributes {
        let (prefix, name) = split_qname(&key)?;
        let namespace = prefix
            .map(|prefix| resolve_prefix(&element.namespaces, prefix))
            .transpose()?;
        // `a:x` and `b:x` with a and b bound to the same namespace are the same attribute
        if element
            .attributes
            .iter()
            .any(|a| a.name == name && a.namespace == namespace)
        {
            return Err(format!("duplicate attribute '{}'", key));
        }
        element.attributes.push(Attribute {
            prefix: prefix.map(str::to_string),
            name: name.to_string(),
            namespace,
            value,
        });
    }
    Ok(element)
}

fn push_text(element: &mut Element, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Node::Text(last)) = element.children.last_mut() {
        last.push_str(text);
    } else {
        element.children.push(Node::Text(text.to_string()));
    }
}

fn predefined_entity(name: &str) -> Option<char> {
    match name {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}

/// Moves the innermost open element to its parent (or makes it the root)
fn close(stack: &mut Vec<Element>, root: &mut Option<Element>) -> Result<(), String> {
    let element = stack.pop().ok_or("unexpected end tag")?;
    match stack.last_mut() {
        Some(parent) => parent.children.push(Node::Element(element)),
        None => *root = Some(element),
    }
    Ok(())
}

/// Parses a document and returns its root element
pub fn parse(xml: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("malformed XML at {}: {}", reader.error_position(), e))?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                if root.is_some() {
                    return Err("content after the root element".to_string());
                }
                if stack.len() >= MAX_DEPTH {
                    return Err("elements are nested too deeply".to_string());
                }
                let element = start_element(start, stack.last().map(|p| &p.namespaces))?;
                stack.push(element);
                if matches!(event, Event::Empty(_)) {
                    close(&mut stack, &mut root)?;
                }
            }
            Event::End(_) => close(&mut stack, &mut root)?,
            Event::Text(text) => match stack.last_mut() {
                Some(element) => push_text(element, &text.xml10_content()),
                None if text.trim().is_empty() => {}
                None => return Err("text outside the root element".to_string()),
            },
            Event::CData(data) => match stack.last_mut() {
                Some(element) => push_text(element, &data.xml10_content()),
                None => return Err("CDATA outside the root element".to_string()),
            },
            Event::GeneralRef(reference) => {
                let element = stack
                    .last_mut()
                    .ok_or("reference outside the root element")?;
                let resolved = match reference
                    .resolve_char_ref()
                    .map_err(|e| format!("invalid character reference: {}", e))?
                {
                    Some(c) => c,
                    None => {
                        let name: &str = &reference;
                        predefined_entity(name)
                            .ok_or_else(|| format!("undefined entity '&{};'", name))?
                    }
                };
                push_text(element, resolved.encode_utf8(&mut [0; 4]));
            }
            Event::Comment(_) | Event::Decl(_) => {}
            Event::DocType(_) => return Err("DTDs are not allowed".to_string()),
            Event::PI(_) => return Err("processing instructions are not allowed".to_string()),
            Event::Eof => break,
        }
    }
    if !stack.is_empty() {
        return Err("unclosed element".to_string());
    }
    root.ok_or_else(|| "no root element".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_keeps_prefixes_and_resolves_namespaces() {
        let root = parse(
            r#"<?xml version="1.0"?>
<p:Response xmlns:p="urn:p" xmlns="urn:d" ID="_1"><Item a:x="1" xmlns:a="urn:a">x &amp; y<!-- c -->&#x41;<![CDATA[<z>]]></Item></p:Response>"#,
        )
        .unwrap();
        assert!(root.is("urn:p", "Response"));
        assert_eq!(root.prefix.as_deref(), Some("p"));
        assert_eq!(root.attr("ID"), Some("_1"));
        let item = root.child_elements().next().unwrap();
        assert!(item.is("urn:d", "Item"));
        assert_eq!(item.prefix, None);
        assert_eq!(item.attributes[0].namespace.as_deref(), Some("urn:a"));
        assert_eq!(item.text(), "x & yA<z>");
        assert_eq!(item.namespaces.len(), 3);
    }

    #[test]
    fn test_attribute_values_and_line_ends_are_normalized() {
        let root = parse("<a v=\"1\n2\t3&#xA;4\">x\r\ny</a>").unwrap();
        assert_eq!(root.attr("v"), Some("1 2 3\n4"));
        assert_eq!(root.text(), "x\ny");
    }

    #[test]
    fn test_dangerous_or_malformed_input_is_refused() {
        for xml in [
            r#"<!DOCTYPE a [<!ENTITY e "x">]><a>&e;</a>"#,
            "<a>&e;</a>",
            "<?pi x?><a/>",
            "<a><?pi x?></a>",
            "<p:a/>",
            r#"<a x="1" x="2"/>"#,
            r#"<a xmlns:p="urn:x" xmlns:q="urn:x" p:x="1" q:x="2"/>"#,
            "<a></b>",
            "<a/><b/>",
            "<a>",
            "text<a/>",
            "",
        ] {
            assert!(parse(xml).is_err(), "{}", xml);
        }
        let deep = format!("{}{}", "<a>".repeat(100), "</a>".repeat(100));
        assert!(parse(&deep).is_err());
    }
}
