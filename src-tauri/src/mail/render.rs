use std::borrow::Cow;
use std::collections::HashSet;

use ammonia::{Builder, UrlRelative};

const CLEAN_CONTENT_TAGS: &[&str] = &[
    "script", "style", "form", "input", "button", "textarea", "select", "option", "iframe",
    "object", "embed", "link", "meta", "base",
];

const ALLOWED_SCHEMES: &[&str] = &["http", "https", "mailto", "data"];

const ALLOWED_STYLE_PROPERTIES: &[&str] = &[
    "background",
    "background-color",
    "background-image",
    "background-position",
    "background-repeat",
    "background-size",
    "border",
    "border-bottom",
    "border-bottom-color",
    "border-bottom-left-radius",
    "border-bottom-right-radius",
    "border-bottom-style",
    "border-bottom-width",
    "border-collapse",
    "border-color",
    "border-left",
    "border-left-color",
    "border-left-style",
    "border-left-width",
    "border-radius",
    "border-right",
    "border-right-color",
    "border-right-style",
    "border-right-width",
    "border-spacing",
    "border-style",
    "border-top",
    "border-top-color",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-top-style",
    "border-top-width",
    "border-width",
    "box-shadow",
    "box-sizing",
    "clear",
    "color",
    "direction",
    "display",
    "float",
    "font",
    "font-family",
    "font-size",
    "font-style",
    "font-variant",
    "font-weight",
    "height",
    "letter-spacing",
    "line-height",
    "list-style",
    "list-style-position",
    "list-style-type",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "max-height",
    "max-width",
    "min-height",
    "min-width",
    "mso-hide",
    "mso-line-height-rule",
    "mso-padding-alt",
    "mso-table-lspace",
    "mso-table-rspace",
    "opacity",
    "outline",
    "overflow",
    "overflow-wrap",
    "padding",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "page-break-after",
    "page-break-before",
    "table-layout",
    "text-align",
    "text-decoration",
    "text-indent",
    "text-transform",
    "vertical-align",
    "visibility",
    "white-space",
    "width",
    "word-break",
    "word-spacing",
    "word-wrap",
    "-webkit-font-smoothing",
    "-webkit-text-size-adjust",
];

const MAX_STYLESHEET_BYTES: usize = 256 * 1024;

fn extract_style_blocks(raw: &str) -> (String, Vec<&str>) {
    let lower = raw.to_ascii_lowercase();
    let bytes = raw.as_bytes();
    let mut markup = String::with_capacity(raw.len());
    let mut styles = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = lower[cursor..].find("<style") {
        let start = cursor + offset;
        let after = start + 6;
        if !matches!(bytes.get(after), Some(b' ' | b'\t' | b'\r' | b'\n' | b'>')) {
            cursor = after;
            continue;
        }
        let Some(open_end) = lower[start..].find('>').map(|end| start + end + 1) else {
            break;
        };
        let Some(close) = lower[open_end..].find("</style").map(|end| open_end + end) else {
            break;
        };
        markup.push_str(&raw[cursor..start]);
        styles.push(&raw[open_end..close]);
        let Some(tag_end) = lower[close..].find('>').map(|end| close + end + 1) else {
            break;
        };
        cursor = tag_end;
    }
    markup.push_str(&raw[cursor..]);
    (markup, styles)
}

fn sanitize_stylesheet(css: &str) -> Option<String> {
    let css = css.trim();
    if css.is_empty() || css.len() > MAX_STYLESHEET_BYTES {
        return None;
    }
    let lower = css.to_ascii_lowercase();
    let blocked = [
        "@import",
        "@charset",
        "@namespace",
        "expression(",
        "javascript:",
        "-moz-binding",
        "behavior:",
        "position:fixed",
        "position:sticky",
        "data:image/svg",
    ];
    if blocked.iter().any(|needle| lower.contains(needle)) {
        return None;
    }
    Some(css.to_string())
}

pub fn sanitize_html(raw: &str) -> String {
    let (markup, styles) = extract_style_blocks(raw);
    let cleaned = Builder::default()
        .clean_content_tags(HashSet::from_iter(CLEAN_CONTENT_TAGS.iter().copied()))
        .url_schemes(HashSet::from_iter(ALLOWED_SCHEMES.iter().copied()))
        .url_relative(UrlRelative::Deny)
        .add_generic_attributes(&["style", "class"])
        .add_tags(&["font"])
        .add_tag_attributes("font", &["color", "face", "size"])
        .add_tag_attributes(
            "table",
            &[
                "width",
                "height",
                "bgcolor",
                "cellpadding",
                "cellspacing",
                "border",
                "role",
            ],
        )
        .add_tag_attributes("tbody", &["width", "height", "bgcolor"])
        .add_tag_attributes("thead", &["width", "height", "bgcolor"])
        .add_tag_attributes("tfoot", &["width", "height", "bgcolor"])
        .add_tag_attributes("tr", &["width", "height", "bgcolor", "valign"])
        .add_tag_attributes("td", &["width", "height", "bgcolor", "valign", "nowrap"])
        .add_tag_attributes("th", &["width", "height", "bgcolor", "valign", "nowrap"])
        .add_tag_attributes("body", &["bgcolor", "width"])
        .filter_style_properties(HashSet::from_iter(ALLOWED_STYLE_PROPERTIES.iter().copied()))
        .attribute_filter(|element, attribute, value| {
            if element == "img" && attribute == "src" {
                return safe_image_src(value).then_some(Cow::Borrowed(value));
            }
            if attribute == "srcset" {
                return None;
            }
            if value.trim().to_ascii_lowercase().starts_with("data:") {
                return None;
            }
            Some(Cow::Borrowed(value))
        })
        .clean(&markup)
        .to_string();
    let stylesheet = styles
        .iter()
        .filter_map(|css| sanitize_stylesheet(css))
        .collect::<Vec<_>>()
        .join("\n");
    if stylesheet.is_empty() {
        cleaned
    } else {
        format!("<style>{stylesheet}</style>{cleaned}")
    }
}

fn safe_image_src(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || raster_data_image(&lower)
}

fn raster_data_image(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("data:image/") else {
        return false;
    };
    let Some(end) = rest.find([';', ',']) else {
        return false;
    };
    matches!(
        &rest[..end],
        "png" | "jpeg" | "gif" | "webp" | "bmp" | "avif"
    )
}

fn normalize_content_id(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("cid:")
        .trim_matches(['<', '>'])
        .to_ascii_lowercase()
}

pub fn inline_cid_images(
    html: &str,
    attachments: &[crate::mail::parse::ParsedAttachment],
) -> (String, Vec<String>) {
    use base64::Engine;

    let mut images = std::collections::HashMap::new();
    for attachment in attachments {
        if !attachment.mime_type.starts_with("image/") {
            continue;
        }
        let Some(content_id) = attachment.content_id.as_deref() else {
            continue;
        };
        images.insert(
            normalize_content_id(content_id),
            (attachment.mime_type.clone(), attachment.data.clone()),
        );
    }
    if images.is_empty() {
        return (html.to_string(), Vec::new());
    }

    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut replacements = Vec::new();
    let mut used = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find("<img") {
        let start = cursor + offset;
        let after = start + 4;
        if !matches!(
            bytes.get(after),
            Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')
        ) {
            cursor = after;
            continue;
        }
        let Some(tag_end) = lower[start..].find('>').map(|end| start + end) else {
            break;
        };
        if let Some((value_start, value_end)) = src_attribute(&lower, start + 4, tag_end) {
            let value = &html[value_start..value_end];
            let url = html_escape::decode_html_entities(value).trim().to_string();
            if url.to_ascii_lowercase().starts_with("cid:") {
                let id = normalize_content_id(&url);
                if let Some((mime_type, data)) = images.get(&id) {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
                    replacements.push((
                        value_start,
                        value_end,
                        format!("data:{mime_type};base64,{encoded}"),
                    ));
                    if !used.contains(&id) {
                        used.push(id);
                    }
                }
            }
        }
        cursor = tag_end + 1;
    }
    if replacements.is_empty() {
        (html.to_string(), Vec::new())
    } else {
        (replace_ranges(html, &replacements), used)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteImage {
    pub url: String,
    pub start: usize,
    pub end: usize,
}

pub fn remote_image_sources(html: &str) -> Vec<RemoteImage> {
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let mut images = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = lower[cursor..].find("<img") {
        let start = cursor + offset;
        let after = start + 4;
        let boundary = bytes.get(after).copied();
        if !matches!(boundary, Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')) {
            cursor = after;
            continue;
        }
        let Some(tag_end) = lower[start..].find('>').map(|end| start + end) else {
            break;
        };
        if let Some((value_start, value_end)) = src_attribute(&lower, start + 4, tag_end) {
            let value = &html[value_start..value_end];
            let url = html_escape::decode_html_entities(value).trim().to_string();
            let scheme = url.to_ascii_lowercase();
            if scheme.starts_with("http://") || scheme.starts_with("https://") {
                images.push(RemoteImage {
                    url,
                    start: value_start,
                    end: value_end,
                });
            }
        }
        cursor = tag_end + 1;
    }
    images
}

fn src_attribute(lower: &str, mut cursor: usize, end: usize) -> Option<(usize, usize)> {
    let bytes = lower.as_bytes();
    while cursor < end {
        while cursor < end && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < end && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'=' {
            cursor += 1;
        }
        if name_start == cursor {
            cursor += 1;
            continue;
        }
        let name = &lower[name_start..cursor];
        while cursor < end && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= end || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < end && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= end {
            return None;
        }
        let quote = bytes[cursor];
        if quote == b'"' || quote == b'\'' {
            cursor += 1;
            let value_start = cursor;
            while cursor < end && bytes[cursor] != quote {
                cursor += 1;
            }
            if cursor >= end {
                return None;
            }
            if name.eq_ignore_ascii_case("src") {
                return Some((value_start, cursor));
            }
            cursor += 1;
        } else {
            let value_start = cursor;
            while cursor < end && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if name.eq_ignore_ascii_case("src") {
                return Some((value_start, cursor));
            }
        }
    }
    None
}

pub fn replace_ranges(html: &str, replacements: &[(usize, usize, String)]) -> String {
    let mut ranges = replacements.to_vec();
    ranges.sort_by_key(|range| std::cmp::Reverse(range.0));
    let mut output = html.to_string();
    for (start, end, value) in ranges {
        if start <= end && end <= output.len() {
            output.replace_range(start..end, &value);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_scripts_handlers_forms_and_javascript_urls() {
        let clean = sanitize_html(
            r#"<script>alert(1)</script><p onclick="x()">ok</p><form><input></form><a href="javascript:alert(1)">bad</a>"#,
        );
        assert!(!clean.contains("<script"));
        assert!(!clean.contains("onclick"));
        assert!(!clean.contains("<form"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>ok</p>"));
    }

    #[test]
    fn keeps_basic_formatting_and_images() {
        let clean = sanitize_html(
            r#"<p><strong>Hi</strong><img src="https://example.com/a.png" alt="a"></p>"#,
        );
        assert!(clean.contains("<strong>Hi</strong>"));
        assert!(clean.contains("src=\"https://example.com/a.png\""));
    }

    #[test]
    fn preserves_safe_embedded_stylesheets() {
        let clean = sanitize_html(
            r##"<style>@media (prefers-color-scheme: dark){.footer{background-color:#232f3e;color:#ffffff}}</style><div class="footer">Hi</div>"##,
        );
        assert!(clean.starts_with("<style>"));
        assert!(clean.contains("@media"));
        assert!(clean.contains(".footer"));
        assert!(clean.contains("background-color:#232f3e"));
        assert!(clean.contains("class=\"footer\""));
    }

    #[test]
    fn drops_stylesheets_with_dangerous_constructs() {
        let clean = sanitize_html(
            r##"<style>@import url(https://evil.example/x.css);.x{background:url(javascript:alert(1))}</style><p>x</p>"##,
        );
        assert!(!clean.contains("<style"));
        assert!(!clean.contains("@import"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("<p>x</p>"));
    }

    #[test]
    fn preserves_email_table_attributes_and_classes() {
        let clean = sanitize_html(
            r##"<table width="600" bgcolor="#232f3e" cellpadding="0" cellspacing="0" role="presentation"><tr><td bgcolor="#ffffff" width="600" valign="top" class="footer"><font color="#ffffff">Hi</font></td></tr></table>"##,
        );
        assert!(clean.contains("width=\"600\""));
        assert!(clean.contains("bgcolor=\"#232f3e\""));
        assert!(clean.contains("cellpadding=\"0\""));
        assert!(clean.contains("cellspacing=\"0\""));
        assert!(clean.contains("role=\"presentation\""));
        assert!(clean.contains("valign=\"top\""));
        assert!(clean.contains("class=\"footer\""));
        assert!(clean.contains("color=\"#ffffff\""));
    }

    #[test]
    fn preserves_safe_inline_styles_for_email_layout() {
        let clean = sanitize_html(
            r#"<table align="center" style="width:600px;background-color:#ffffff;color:#111111"><tr><td style="padding:16px;font-size:14px">Hi</td></tr></table>"#,
        );
        assert!(clean.contains("width:600px"));
        assert!(clean.contains("background-color:#ffffff"));
        assert!(clean.contains("padding:16px"));
        assert!(clean.contains("font-size:14px"));
    }

    #[test]
    fn removes_unsafe_style_properties_and_urls() {
        let clean = sanitize_html(
            r#"<div style="position:fixed;top:0;background-image:url(javascript:alert(1));color:red"><a href="ftp://example.com">ftp</a><a href="mailto:a@example.com">mail</a></div>"#,
        );
        assert!(!clean.contains("position"));
        assert!(!clean.contains("javascript:"));
        assert!(clean.contains("color:red"));
        assert!(!clean.contains("ftp://"));
        assert!(clean.contains("mailto:a@example.com"));
    }

    #[test]
    fn adds_safe_rel_to_links() {
        let clean = sanitize_html(r#"<a href="https://example.com">site</a>"#);
        assert!(clean.contains("rel=\"noopener noreferrer\""));
    }

    #[test]
    fn keeps_raster_data_images_but_not_svg_or_data_links() {
        let clean = sanitize_html(
            r#"<img src="data:image/png;base64,AAAA"><img src="data:image/svg+xml;base64,AAAA"><a href="data:text/html,x">bad</a>"#,
        );
        assert!(clean.contains("data:image/png;base64,AAAA"));
        assert!(!clean.contains("data:image/svg"));
        assert!(!clean.contains("data:text/html"));
    }

    #[test]
    fn inlines_cid_images_and_reports_used_content_ids() {
        let attachment = crate::mail::parse::ParsedAttachment {
            filename: "logo.png".into(),
            mime_type: "image/png".into(),
            size: 3,
            content_id: Some("<logo@example>".into()),
            is_inline: true,
            data: vec![1, 2, 3],
        };
        let (html, used) = inline_cid_images(
            r#"<p><img src="cid:logo@example" alt="logo"></p>"#,
            &[attachment],
        );
        assert!(html.contains("src=\"data:image/png;base64,"));
        assert_eq!(used, vec!["logo@example".to_string()]);
    }

    #[test]
    fn leaves_unknown_content_ids_untouched() {
        let (html, used) = inline_cid_images(r#"<img src="cid:missing">"#, &[]);
        assert!(html.contains("cid:missing"));
        assert!(used.is_empty());
    }

    #[test]
    fn finds_and_replaces_remote_image_sources() {
        let html = r#"<p><img alt="a" src="https://example.com/a.png"><img src='https://example.com/b.jpg'><img src="data:image/png;base64,AAAA"><img data-src="https://example.com/no.png"></p>"#;
        let images = remote_image_sources(html);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].url, "https://example.com/a.png");
        assert_eq!(images[1].url, "https://example.com/b.jpg");

        let replacements: Vec<_> = images
            .iter()
            .map(|image| {
                (
                    image.start,
                    image.end,
                    "data:image/png;base64,AAAA".to_string(),
                )
            })
            .collect();
        let replaced = replace_ranges(html, &replacements);
        assert!(!replaced.contains("https://example.com/a.png"));
        assert!(replaced.contains("data:image/png;base64,AAAA"));
    }

    #[test]
    fn decodes_entities_in_remote_image_urls() {
        let images = remote_image_sources(r#"<img src="https://example.com/a.png?x=1&amp;y=2">"#);
        assert_eq!(images[0].url, "https://example.com/a.png?x=1&y=2");
    }

    #[test]
    fn removes_srcset_and_relative_urls() {
        let clean = sanitize_html(
            r#"<img src="/tracking.gif" srcset="https://example.com/a.png 2x"><a href="/local">local</a>"#,
        );
        assert!(!clean.contains("srcset"));
        assert!(!clean.contains("/tracking.gif"));
        assert!(!clean.contains("/local"));
    }
}
