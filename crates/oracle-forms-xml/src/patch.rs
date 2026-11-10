//! Lossless edits to Forms2XML documents.
//!
//! The original XML is kept as the source of truth. An edit changes only the attributes
//! it names, on the one `<Item>` element it targets; every other byte of the document is
//! copied unchanged. An unedited document therefore round-trips byte-for-byte, and
//! properties this tool does not model (fonts, colours, LOV settings, triggers, ...) are
//! never lost.

use quick_xml::events::Event;
use quick_xml::Reader;

/// New attribute values for one item, identified by block and item name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPatch {
    pub block: String,
    pub item: String,
    /// `(attribute, new value)` with the value unescaped (it is escaped on write).
    pub attributes: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PatchError {
    #[error("XML could not be read: {0}")]
    Xml(String),
    #[error("Item {block}.{item} was not found in the XML")]
    ItemNotFound { block: String, item: String },
}

fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// Sets `name="value"` inside one start tag (`<Item ...>` or `<Item .../>`), replacing
/// the existing attribute if present or appending it otherwise.
fn set_attribute(tag: &str, name: &str, value: &str) -> String {
    let escaped = escape_attr(value);
    // Find ` name="` as a whole attribute name (preceded by whitespace).
    let bytes = tag.as_bytes();
    let needle = format!("{}=", name);
    let mut search = 0;
    while let Some(rel) = tag[search..].find(&needle) {
        let at = search + rel;
        let preceded_by_space = at > 0 && (bytes[at - 1] as char).is_whitespace();
        let quote_pos = at + needle.len();
        if preceded_by_space && quote_pos < tag.len() {
            let quote = bytes[quote_pos] as char;
            if quote == '"' || quote == '\'' {
                if let Some(end_rel) = tag[quote_pos + 1..].find(quote) {
                    let end = quote_pos + 1 + end_rel;
                    return format!("{}\"{}\"{}", &tag[..quote_pos], escaped, &tag[end + 1..]);
                }
            }
        }
        search = at + needle.len();
    }
    // Not present: insert before `/>` or `>`.
    let insert_at = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
    let trimmed = tag[..insert_at].trim_end();
    format!("{} {}=\"{}\"{}", trimmed, name, escaped, &tag[insert_at..])
}

fn name_attr(e: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == b"Name").and_then(|a| a.unescape_value().ok().map(|v| v.to_string()))
}

/// Applies item patches to a Forms2XML document, changing only the targeted attributes.
pub fn patch_items(xml: &str, patches: &[ItemPatch]) -> Result<String, PatchError> {
    if patches.is_empty() {
        return Ok(xml.to_string());
    }
    let mut reader = Reader::from_str(xml);
    let mut block_stack: Vec<Option<String>> = Vec::new();
    // (start, end, replacement) spans, in document order.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut applied = vec![false; patches.len()];

    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| PatchError::Xml(e.to_string()))?;
        let end = reader.buffer_position() as usize;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let is_empty = matches!(event, Event::Empty(_));
                let local = e.local_name();
                let tag_name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                let current_block = block_stack.iter().rev().flatten().next().cloned();
                if tag_name == "Item" {
                    if let (Some(block), Some(item)) = (current_block.as_deref(), name_attr(e)) {
                        for (i, p) in patches.iter().enumerate() {
                            if !applied[i] && p.block.eq_ignore_ascii_case(block) && p.item.eq_ignore_ascii_case(&item) {
                                let mut tag = xml[start..end].to_string();
                                for (attr, value) in &p.attributes {
                                    tag = set_attribute(&tag, attr, value);
                                }
                                edits.push((start, end, tag));
                                applied[i] = true;
                            }
                        }
                    }
                }
                if !is_empty {
                    block_stack.push(if tag_name == "Block" { name_attr(e) } else { None });
                }
            }
            Event::End(_) => {
                block_stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if let Some(i) = applied.iter().position(|a| !a) {
        return Err(PatchError::ItemNotFound { block: patches[i].block.clone(), item: patches[i].item.clone() });
    }

    let mut out = String::with_capacity(xml.len() + 64);
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        out.push_str(&xml[cursor..start]);
        out.push_str(&replacement);
        cursor = end;
    }
    out.push_str(&xml[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<?xml version = '1.0' encoding = 'UTF-8'?>
<Module xmlns="http://xmlns.oracle.com/Forms">
   <FormModule Name="F">
      <Block Name="EMP">
         <Item Name="ENAME" XPosition="10" FontName="Segoe UI" Prompt="Name"/>
         <Item Name="SAL" XPosition="10">
            <Trigger Name="WHEN-VALIDATE-ITEM" TriggerText="IF :EMP.SAL &lt; 0 THEN RAISE FORM_TRIGGER_FAILURE; END IF;"/>
         </Item>
      </Block>
      <Block Name="DEPT">
         <Item Name="ENAME" XPosition="99"/>
      </Block>
   </FormModule>
</Module>
"#;

    #[test]
    fn no_patches_is_byte_identical() {
        assert_eq!(patch_items(DOC, &[]).unwrap(), DOC);
    }

    #[test]
    fn changes_only_the_targeted_attributes() {
        let out = patch_items(
            DOC,
            &[ItemPatch { block: "EMP".into(), item: "ENAME".into(), attributes: vec![("XPosition".into(), "42".into()), ("Prompt".into(), "Full \"name\"".into())] }],
        )
        .unwrap();
        assert!(out.contains(r#"<Item Name="ENAME" XPosition="42" FontName="Segoe UI" Prompt="Full &quot;name&quot;"/>"#), "{out}");
        // Same item name in another block is untouched; everything else identical.
        assert!(out.contains(r#"<Item Name="ENAME" XPosition="99"/>"#));
        assert_eq!(out.len() as isize - DOC.len() as isize, ("42".len() as isize - 2) + ("Full &quot;name&quot;".len() as isize - "Name".len() as isize));
    }

    #[test]
    fn adds_missing_attribute_and_keeps_children() {
        let out = patch_items(DOC, &[ItemPatch { block: "EMP".into(), item: "SAL".into(), attributes: vec![("Width".into(), "80".into())] }]).unwrap();
        assert!(out.contains(r#"<Item Name="SAL" XPosition="10" Width="80">"#), "{out}");
        assert!(out.contains("TriggerText=\"IF :EMP.SAL &lt; 0 THEN"));
    }

    #[test]
    fn unknown_item_is_an_error() {
        let err = patch_items(DOC, &[ItemPatch { block: "EMP".into(), item: "NOPE".into(), attributes: vec![] }]).unwrap_err();
        assert!(matches!(err, PatchError::ItemNotFound { .. }));
    }
}
