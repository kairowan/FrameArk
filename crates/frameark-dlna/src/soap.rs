//! Namespace-aware SOAP input boundary. No external entities or network I/O.

use super::*;

const ENVELOPE: &str = "http://schemas.xmlsoap.org/soap/envelope/";

pub(super) fn parse_soap_action(header: &str, bytes: &[u8]) -> Result<SoapAction, DlnaError> {
    let invalid = || DlnaError::Invalid("SOAP envelope");
    if bytes.len() > MAX_BODY_BYTES {
        return Err(DlnaError::TooLarge("SOAP body"));
    }
    let header = header.trim().trim_matches('"');
    let (service, name) = header.rsplit_once('#').ok_or_else(invalid)?;
    validate_field(service, "SOAP service")?;
    validate_field(name, "SOAP action")?;
    if !matches!(
        service,
        AVTRANSPORT_SERVICE | RENDERING_CONTROL_SERVICE | CONNECTION_MANAGER_SERVICE
    ) {
        return Err(invalid());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    // Also reject an empty DTD, which the XML library intentionally permits.
    if text.contains("<!DOCTYPE") {
        return Err(invalid());
    }
    let document = roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 128,
            entity_resolver: None,
        },
    )
    .map_err(|_| invalid())?;
    let root = document.root_element();
    if !root.has_tag_name((ENVELOPE, "Envelope")) {
        return Err(invalid());
    }
    let children: Vec<_> = root.children().filter(|n| n.is_element()).collect();
    let body = match children.as_slice() {
        [body] => *body,
        [header, body]
            if header.has_tag_name((ENVELOPE, "Header"))
                && !header.children().any(|n| {
                    n.is_element()
                        || (n.is_text() && !n.text().unwrap_or_default().trim().is_empty())
                }) =>
        {
            *body
        }
        _ => return Err(invalid()),
    };
    if !body.has_tag_name((ENVELOPE, "Body")) {
        return Err(invalid());
    }
    let mut actions = body.children().filter(|n| n.is_element());
    let action = actions.next().ok_or_else(invalid)?;
    if actions.next().is_some() || !action.has_tag_name((service, name)) {
        return Err(invalid());
    }
    for container in [root, body, action] {
        if container
            .children()
            .any(|n| n.is_text() && !n.text().unwrap_or_default().trim().is_empty())
        {
            return Err(invalid());
        }
    }
    let mut args = BTreeMap::new();
    for node in action.children().filter(|n| n.is_element()) {
        if node.children().any(|n| n.is_element())
            || node.tag_name().namespace().is_some_and(|ns| ns != service)
            || node.attributes().len() != 0
        {
            return Err(invalid());
        }
        let value: String = node
            .children()
            .filter_map(|n| n.text().filter(|_| n.is_text()))
            .collect();
        if value.len() > MAX_SOAP_ARGUMENT_BYTES {
            return Err(DlnaError::TooLarge("SOAP argument"));
        }
        if !eventing::valid_xml_text(&value)
            || args
                .insert(node.tag_name().name().to_string(), value)
                .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(SoapAction {
        service: service.to_string(),
        name: name.to_string(),
        args,
    })
}
