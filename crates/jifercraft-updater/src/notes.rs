//! Release notes arrive as HTML or Markdown-ish text from the release feed; the UI shows plain text.

const MAX_CHARS: usize = 4000;
const BLOCK_ENDS: [&str; 11] = ["p", "div", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "tr"];
const MORE_BLOCK_ENDS: [&str; 2] = ["pre", "blockquote"];

/// Plain text for display: tags dropped (their text kept), lists bulleted, entities decoded exactly once,
/// whitespace tidied and the result capped. `None` when nothing readable remains.
pub fn notes_to_text(raw: &str) -> Option<String> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let without_hidden = remove_blocks(&normalized);
    let text = decode_entities(&strip_tags(&without_hidden));

    let mut lines: Vec<String> = text.split('\n').map(collapse_spaces).collect();
    lines.dedup_by(|current, previous| current.is_empty() && previous.is_empty());
    let joined = lines.join("\n");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() > MAX_CHARS {
        let head: String = trimmed.chars().take(MAX_CHARS - 1).collect();
        return Some(format!("{}\u{2026}", head.trim_end()));
    }
    Some(trimmed.to_owned())
}

/// Drops comments and the contents of `<script>`/`<style>` elements.
fn remove_blocks(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = [("<!--", "-->"), ("<script", "</script"), ("<style", "</style")]
            .into_iter()
            .filter_map(|(open, close)| lower.find(open).map(|at| (at, close)))
            .min_by_key(|(at, _)| *at);
        let Some((start, close)) = next else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..start]);
        match lower[start..].find(close) {
            Some(offset) => {
                let end = start + offset + close.len();
                let end = if close == "-->" {
                    end
                } else {
                    lower[end..].find('>').map_or(lower.len(), |gt| end + gt + 1)
                };
                rest = &rest[end..];
            }
            None => return out,
        }
    }
}

fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let starts_tag = after
            .trim_start_matches('/')
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '!');
        match after.find('>') {
            Some(end) if starts_tag => {
                out.push_str(replacement(&after[..end]));
                rest = &after[end + 1..];
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn replacement(tag: &str) -> &'static str {
    let closing = tag.starts_with('/');
    let name: String = tag
        .trim_start_matches('/')
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    match (closing, name.as_str()) {
        (false, "li") => "\n\u{2022} ",
        (_, "br") => "\n",
        (true, name) if BLOCK_ENDS.contains(&name) || MORE_BLOCK_ENDS.contains(&name) => "\n",
        _ => "",
    }
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| decode(&after[..end]).map(|c| (c, end)));
        if let Some((character, end)) = decoded {
            out.push(character);
            rest = &after[end + 1..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn decode(entity: &str) -> Option<char> {
    if let Some(number) = entity.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code).filter(|c| *c != '\0');
    }
    match entity.to_ascii_lowercase().as_str() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => None,
    }
}

fn collapse_spaces(line: &str) -> String {
    line.split([' ', '\t'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_lists_paragraphs_and_breaks_into_text() {
        let html = "<h2>What's new</h2><p>Faster launches.</p><ul><li>Instance drawer</li><li>Per-instance RAM</li></ul>Done<br>Bye";
        assert_eq!(
            notes_to_text(html).unwrap(),
            "What's new\nFaster launches.\n\n\u{2022} Instance drawer\n\u{2022} Per-instance RAM\nDone\nBye"
        );
    }

    #[test]
    fn drops_scripts_styles_comments_and_tags_but_keeps_text() {
        let html = "<style>p{color:red}</style><!-- hidden --><p>Hello <b>world</b></p><SCRIPT>alert(\"x\")</SCRIPT><a href=\"https://example.test\">link</a>";
        assert_eq!(notes_to_text(html).unwrap(), "Hello world\nlink");
    }

    #[test]
    fn decodes_entities_exactly_once() {
        assert_eq!(
            notes_to_text("Fish &amp; chips &lt;b&gt;bold&lt;/b&gt; &#169; &#x1F600; &nbsp;end").unwrap(),
            "Fish & chips <b>bold</b> \u{a9} \u{1F600} end"
        );
        assert_eq!(notes_to_text("&amp;lt;").unwrap(), "&lt;");
    }

    #[test]
    fn leaves_lone_brackets_and_unknown_entities() {
        assert_eq!(notes_to_text("1 < 2 &unknown; ok").unwrap(), "1 < 2 &unknown; ok");
        assert_eq!(notes_to_text("a <3 b").unwrap(), "a <3 b");
    }

    #[test]
    fn empty_notes_are_none_and_long_ones_are_capped() {
        assert_eq!(notes_to_text("<p> </p>"), None);
        assert_eq!(notes_to_text(""), None);
        let capped = notes_to_text(&"x".repeat(5000)).unwrap();
        assert_eq!(capped.chars().count(), MAX_CHARS);
        assert!(capped.ends_with('\u{2026}'));
    }

    #[test]
    fn keeps_markdown_feeds_readable() {
        assert_eq!(
            notes_to_text("## Changes\r\n\r\n\r\n\r\n- Faster\t\tlaunches").unwrap(),
            "## Changes\n\n- Faster launches"
        );
    }
}
