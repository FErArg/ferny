//! Markdown preview (H4.2b, threats T6, T7): CommonMark to HTML for a
//! `QTextBrowser`. Raw HTML in the document becomes text, images become
//! `[image: alt]` (nothing is ever loaded), and links keep their target
//! only for `http`, `https`, `mailto` and `#` anchors.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Largest document previewed, in bytes.
pub const MAX_MARKDOWN_BYTES: usize = 2 * 1024 * 1024;

fn allowed(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    ["http://", "https://", "mailto:", "#"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

/// The HTML of `text`; `None` if it is too large to preview.
pub fn to_html(text: &str) -> Option<String> {
    if text.len() > MAX_MARKDOWN_BYTES {
        return None;
    }
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(text, options).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        Event::Start(Tag::Image { .. }) => Event::Text(CowStr::Borrowed("[image: ")),
        Event::End(TagEnd::Image) => Event::Text(CowStr::Borrowed("]")),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: if allowed(&dest_url) {
                dest_url
            } else {
                CowStr::Borrowed("")
            },
            title,
            id,
        }),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    Some(out)
}
