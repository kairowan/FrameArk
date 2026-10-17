//! DIDL-Lite sidecars: metadata is a declaration, never authorization to fetch.
use super::*;

const DIDL: &str = "urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/";
const UPNP: &str = "urn:schemas-upnp-org:metadata-1-0/upnp/";

/// One explicitly declared sidecar. No Debug: URI and labels may be private.
pub struct SubtitleResource {
    /// Absolute HTTP(S) address. The host must independently authorize it.
    pub uri: String,
    /// Declared supported subtitle MIME type; must match the fetched source.
    pub mime_type: String,
    /// Bounded language label, or empty when not provided.
    pub language: String,
}

/// Reads at most four SRT/WebVTT resources from one bounded DIDL item.
/// Accepts direct `res` sidecars and ContentDirectory:4 Caption components.
/// Nonempty metadata must be well-formed DIDL-Lite. Unknown namespaced elements
/// are ignored, never fetched. Multiple items or unmatched primary resources
/// with sidecars are rejected instead of guessing a media-to-caption binding.
pub fn subtitle_resources(
    metadata: &str,
    media_uri: &str,
) -> Result<Vec<SubtitleResource>, DlnaError> {
    if metadata.is_empty() {
        return Ok(Vec::new());
    }
    if metadata.len() > MAX_RENDERER_METADATA_BYTES {
        return Err(DlnaError::TooLarge("subtitle metadata"));
    }
    soap::check_depth(metadata, 8)?;
    let doc = roxmltree::Document::parse_with_options(
        metadata,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 128,
            entity_resolver: None,
        },
    )
    .map_err(|_| DlnaError::Invalid("subtitle metadata"))?;
    let root = doc.root_element();
    if !root.has_tag_name((DIDL, "DIDL-Lite")) {
        return Err(DlnaError::Invalid("subtitle DIDL root"));
    }
    let items: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name((DIDL, "item")))
        .collect();
    if items.len() != 1 {
        return Err(DlnaError::Invalid("subtitle DIDL item"));
    }
    let item = items[0];
    let mut resources = Vec::new();
    let mut primary = false;
    let mut primary_ids = Vec::new();
    for res in item.children().filter(|n| n.has_tag_name((DIDL, "res"))) {
        let uri = scalar(res)?;
        if uri == media_uri {
            primary = true;
            if let Some(id) = res.attribute("id") {
                primary_ids.push(id);
            }
            continue;
        }
        append(&mut resources, res, "")?;
    }
    let groups = item
        .children()
        .filter(|n| {
            n.has_tag_name((UPNP, "resExt"))
                && n.attribute("id")
                    .is_some_and(|id| primary_ids.contains(&id))
        })
        .flat_map(|ext| {
            ext.children()
                .filter(|n| n.has_tag_name((UPNP, "componentInfo")))
        })
        .flat_map(|info| {
            info.children()
                .filter(|n| n.has_tag_name((UPNP, "componentGroup")))
        });
    for group in groups {
        for component in group
            .children()
            .filter(|n| n.has_tag_name((UPNP, "component")))
        {
            let classes = component
                .children()
                .filter(|n| n.has_tag_name((UPNP, "componentClass")))
                .map(scalar)
                .collect::<Result<Vec<_>, _>>()?;
            if classes.as_slice() != ["Caption"] {
                continue;
            }
            let language = component
                .children()
                .find(|n| n.has_tag_name((UPNP, "language")))
                .map(scalar)
                .transpose()?
                .unwrap_or_default();
            for container in component
                .children()
                .filter(|n| n.has_tag_name((UPNP, "compRes")))
            {
                for res in container
                    .children()
                    .filter(|n| n.has_tag_name((UPNP, "res")))
                {
                    append(&mut resources, res, &language)?;
                }
            }
        }
    }
    if !resources.is_empty() && !primary {
        return Err(DlnaError::Invalid("subtitle primary binding"));
    }
    Ok(resources)
}

fn scalar(node: roxmltree::Node<'_, '_>) -> Result<String, DlnaError> {
    if node.children().any(|n| n.is_element()) {
        return Err(DlnaError::Invalid("subtitle scalar"));
    }
    Ok(node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .trim()
        .to_owned())
}

fn append(
    output: &mut Vec<SubtitleResource>,
    node: roxmltree::Node<'_, '_>,
    language: &str,
) -> Result<(), DlnaError> {
    let Some(info) = node.attribute("protocolInfo") else {
        return Ok(());
    };
    let fields: Vec<_> = info.splitn(4, ':').collect();
    if fields.len() != 4 || fields[0] != "http-get" || fields[1] != "*" {
        return Ok(());
    }
    let mime = fields[2].to_ascii_lowercase();
    if !matches!(
        mime.as_str(),
        "text/srt" | "application/x-subrip" | "text/vtt"
    ) {
        return Ok(());
    }
    let uri = scalar(node)?;
    validate_media_uri(&uri).map_err(|_| DlnaError::Invalid("subtitle URI"))?;
    if output.len() >= 4 || output.iter().any(|r| r.uri == uri) {
        return Err(DlnaError::TooLarge("subtitle resources"));
    }
    let language = node
        .attribute(("http://www.w3.org/XML/1998/namespace", "lang"))
        .unwrap_or(language);
    if language.len() > 35
        || !language
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(DlnaError::Invalid("subtitle language"));
    }
    output.push(SubtitleResource {
        uri,
        mime_type: mime,
        language: language.to_string(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const MEDIA: &str = "http://192.168.1.2/movie.mp4";
    fn wrap(content: &str) -> String {
        format!(
            "<DIDL-Lite xmlns=\"{DIDL}\" xmlns:u=\"{UPNP}\"><item><res id=\"main\" protocolInfo=\"http-get:*:video/mp4:*\">{MEDIA}</res>{content}</item></DIDL-Lite>"
        )
    }
    fn sidecar(index: usize) -> String {
        format!(
            "<res protocolInfo=\"http-get:*:text/srt:*\" xml:lang=\"zh-CN\">http://192.168.1.2/sub{index}.srt</res>"
        )
    }

    #[test]
    fn direct_and_bound_component_caption_resources() {
        let document = wrap(&format!(
            "{}<u:resExt id=\"main\"><u:componentInfo><u:componentGroup><u:component><u:componentClass>Caption</u:componentClass><u:language>en</u:language><u:compRes><u:res protocolInfo=\"http-get:*:text/vtt:*\">http://192.168.1.2/<!--ignored-->english.vtt?x=1&amp;y=2</u:res></u:compRes></u:component></u:componentGroup></u:componentInfo></u:resExt>",
            sidecar(1)
        ));
        let result = subtitle_resources(&document, MEDIA).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].language, "zh-CN");
        assert_eq!(result[1].language, "en");
        assert_eq!(result[1].mime_type, "text/vtt");
        assert_eq!(result[1].uri, "http://192.168.1.2/english.vtt?x=1&y=2");
        let unmatched = document.replace("resExt id=\"main\"", "resExt id=\"other\"");
        assert_eq!(subtitle_resources(&unmatched, MEDIA).unwrap().len(), 1);
        assert!(subtitle_resources(&document, "http://different/movie.mp4").is_err());
    }

    #[test]
    fn namespaces_unknown_metadata_and_no_implicit_sidecar_fetch() {
        assert!(subtitle_resources("", MEDIA).unwrap().is_empty());
        let doc = wrap(
            "<title xmlns=\"unknown\">title</title><res xmlns=\"unknown\" protocolInfo=\"http-get:*:text/vtt:*\">http://evil/caption</res><res protocolInfo=\"http-get:*:text/html:*\">http://evil/html</res>",
        );
        assert!(subtitle_resources(&doc, MEDIA).unwrap().is_empty());
        assert!(subtitle_resources("<DIDL-Lite><item/></DIDL-Lite>", MEDIA).is_err());
    }

    #[test]
    fn duplicates_nested_scalars_and_resource_limits_fail() {
        let four = (0..4).map(sidecar).collect::<String>();
        assert_eq!(subtitle_resources(&wrap(&four), MEDIA).unwrap().len(), 4);
        for content in [
            format!("{four}{}", sidecar(4)),
            format!("{0}{0}", sidecar(0)),
            sidecar(0).replace("zh-CN", "bad label"),
            sidecar(0).replace("http://192.168.1.2/sub0.srt", "file:///etc/passwd"),
            sidecar(0).replace("sub0.srt", "<nested/>sub0.srt"),
        ] {
            assert!(subtitle_resources(&wrap(&content), MEDIA).is_err());
        }
        assert!(
            subtitle_resources(
                &wrap(&sidecar(0)).replace("</item>", "</item><item/>"),
                MEDIA
            )
            .is_err()
        );
    }

    #[test]
    fn dtd_depth_node_and_byte_limits_fail_before_recursive_dom() {
        for text in [
            "<!DOCTYPE DIDL-Lite [<!ENTITY x SYSTEM 'file:///private'>]><DIDL-Lite/>",
            &"<x>".repeat(100),
            &"x".repeat(MAX_RENDERER_METADATA_BYTES + 1),
        ] {
            assert!(subtitle_resources(text, MEDIA).is_err());
        }
        assert!(subtitle_resources(&wrap(&"<x/>".repeat(128)), MEDIA).is_err());
    }
}
