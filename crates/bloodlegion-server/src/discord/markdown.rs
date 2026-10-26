//! Discord's Markdown, rendered to HTML for the site. Discord's flavour is not CommonMark (`__x__`
//! underlines, `||x||` hides a spoiler, `-#` is small print, `<t:…>` is a timestamp, mentions are
//! ids), so a general renderer would get it wrong; this covers what Discord shows.
//!
//! Safe to put on a page as is: every piece of the source is escaped, the only markup is the tags
//! written here, and links are kept only for `http`/`https` URLs.

use std::collections::HashMap;

use time::OffsetDateTime;

/// The names a post's mentions had when it was posted or last edited, by id: a mention is an id
/// in the source, and the name behind it can change or go away.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mentions {
    #[serde(default)]
    pub users: HashMap<String, String>,
    #[serde(default)]
    pub roles: HashMap<String, String>,
    #[serde(default)]
    pub channels: HashMap<String, String>,
}

/// `source` as HTML.
pub fn render(source: &str, mentions: &Mentions) -> String {
    let mut out = String::new();
    // Code blocks first: nothing inside one is Markdown. An unclosed fence is plain text.
    let mut rest = source;
    while let Some(start) = rest.find("```")
        && let Some(length) = rest[start + 3..].find("```")
    {
        blocks(&rest[..start], mentions, &mut out);
        code_block(&rest[start + 3..start + 3 + length], &mut out);
        rest = &rest[start + 3 + length + 3..];
    }
    blocks(rest, mentions, &mut out);
    out
}

fn code_block(inner: &str, out: &mut String) {
    // A first line that is one word (```rust) names the language; the code starts after it.
    let code = match inner.split_once('\n') {
        Some((first, code))
            if !first.is_empty()
                && first
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-#._".contains(c)) =>
        {
            code
        }
        _ => inner,
    };
    let code = code.strip_prefix('\n').unwrap_or(code);
    let code = code.strip_suffix('\n').unwrap_or(code);
    out.push_str("<pre><code>");
    escape_into(code, out);
    out.push_str("</code></pre>");
}

/// The line-level structure: paragraphs, headings, small print, lists, and quotes.
fn blocks(text: &str, mentions: &Mentions, out: &mut String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // `>>>` quotes everything after it.
        if let Some(first) = line.strip_prefix(">>> ").or((line == ">>>").then_some("")) {
            flush(&mut paragraph, mentions, out);
            let mut quoted = vec![first];
            quoted.extend(&lines[i + 1..]);
            out.push_str("<blockquote>");
            blocks(&quoted.join("\n"), mentions, out);
            out.push_str("</blockquote>");
            return;
        }
        if let Some(first) = quote_line(line) {
            flush(&mut paragraph, mentions, out);
            let mut quoted = vec![first];
            i += 1;
            while let Some(next) = lines.get(i).and_then(|line| quote_line(line)) {
                quoted.push(next);
                i += 1;
            }
            out.push_str("<blockquote>");
            flush(&mut quoted, mentions, out);
            out.push_str("</blockquote>");
            continue;
        }
        if let Some((level, heading)) = heading(line) {
            flush(&mut paragraph, mentions, out);
            // Headings sit inside a post on a page that has its own, so they start at h3.
            let tag = ["h3", "h4", "h5"][level - 1];
            out.push_str(&format!("<{tag}>"));
            inline(heading, mentions, out);
            out.push_str(&format!("</{tag}>"));
            i += 1;
            continue;
        }
        if let Some(small) = line.strip_prefix("-# ") {
            flush(&mut paragraph, mentions, out);
            out.push_str("<p><small>");
            inline(small, mentions, out);
            out.push_str("</small></p>");
            i += 1;
            continue;
        }
        if let Some((ordered, start, _)) = list_item(line) {
            flush(&mut paragraph, mentions, out);
            if ordered {
                if start == 1 {
                    out.push_str("<ol>");
                } else {
                    out.push_str(&format!("<ol start=\"{start}\">"));
                }
            } else {
                out.push_str("<ul>");
            }
            while let Some((item_ordered, _, item)) = lines.get(i).and_then(|line| list_item(line))
                && item_ordered == ordered
            {
                out.push_str("<li>");
                inline(item, mentions, out);
                out.push_str("</li>");
                i += 1;
            }
            out.push_str(if ordered { "</ol>" } else { "</ul>" });
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut paragraph, mentions, out);
        } else {
            paragraph.push(line);
        }
        i += 1;
    }
    flush(&mut paragraph, mentions, out);
}

/// Writes out the lines gathered so far as a paragraph, one line break per line.
fn flush(lines: &mut Vec<&str>, mentions: &Mentions, out: &mut String) {
    if lines.iter().all(|line| line.trim().is_empty()) {
        lines.clear();
        return;
    }
    out.push_str("<p>");
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            out.push_str("<br>");
        }
        inline(line, mentions, out);
    }
    out.push_str("</p>");
    lines.clear();
}

fn quote_line(line: &str) -> Option<&str> {
    line.strip_prefix("> ").or((line == ">").then_some(""))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    let text = line[hashes..].strip_prefix(' ')?;
    ((1..=3).contains(&hashes) && !text.trim().is_empty()).then_some((hashes, text))
}

/// `(ordered, number, text)` for a list item line: `- x`, `* x`, or `3. x`.
fn list_item(line: &str) -> Option<(bool, u32, &str)> {
    let line = line.trim_start();
    if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some((false, 1, text));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let text = line[digits..].strip_prefix(". ")?;
    let number = line[..digits].parse().ok()?;
    Some((true, number, text))
}

/// The span-level markup within one line.
fn inline(text: &str, mentions: &Mentions, out: &mut String) {
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let consumed = if let Some(c) = rest
            .strip_prefix('\\')
            .and_then(|after| after.chars().next())
            .filter(char::is_ascii_punctuation)
        {
            escape_into(&c.to_string(), out);
            Some(1 + c.len_utf8())
        } else if rest.starts_with('`') {
            code_span(rest, out)
        } else if rest.starts_with('<') {
            angle(rest, mentions, out)
        } else if rest.starts_with('[') {
            masked_link(rest, mentions, out)
        } else if (rest.starts_with("https://") || rest.starts_with("http://"))
            && !text[..i].ends_with(|c: char| c.is_alphanumeric())
        {
            Some(autolink(rest, out))
        } else {
            emphasis(text, i, mentions, out)
        };
        match consumed {
            Some(length) => i += length,
            None => {
                let c = rest.chars().next().expect("i is inside the text");
                escape_into(&text[i..i + c.len_utf8()], out);
                i += c.len_utf8();
            }
        }
    }
}

fn code_span(rest: &str, out: &mut String) -> Option<usize> {
    let ticks = rest.bytes().take_while(|&b| b == b'`').count().min(2);
    let fence = &rest[..ticks];
    let end = rest[ticks..].find(fence)?;
    let code = &rest[ticks..ticks + end];
    if code.is_empty() {
        return None;
    }
    out.push_str("<code>");
    escape_into(code, out);
    out.push_str("</code>");
    Some(ticks + end + ticks)
}

/// The `<…>` forms: mentions, custom emoji, timestamps, and a link written as `<https://…>`.
fn angle(rest: &str, mentions: &Mentions, out: &mut String) -> Option<usize> {
    let end = rest.find('>')?;
    let token = &rest[1..end];
    if token.is_empty() || token.contains(char::is_whitespace) {
        return None;
    }
    let is_id = |id: &str| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    let mention = |sigil: &str,
                   names: &HashMap<String, String>,
                   id: &str,
                   fallback: &str,
                   out: &mut String| {
        out.push_str("<span class=\"mention\">");
        out.push_str(sigil);
        escape_into(names.get(id).map_or(fallback, String::as_str), out);
        out.push_str("</span>");
    };
    if let Some(id) = token.strip_prefix("@&").filter(|id| is_id(id)) {
        mention("@", &mentions.roles, id, "unknown-role", out);
    } else if let Some(id) = token
        .strip_prefix("@!")
        .or_else(|| token.strip_prefix('@'))
        .filter(|id| is_id(id))
    {
        mention("@", &mentions.users, id, "unknown-user", out);
    } else if let Some(id) = token.strip_prefix('#').filter(|id| is_id(id)) {
        mention("#", &mentions.channels, id, "unknown-channel", out);
    } else if let Some(emoji) = token.strip_prefix("a:").or_else(|| token.strip_prefix(':')) {
        // A custom emoji, `<:name:id>`: its name stands in for the image.
        let (name, id) = emoji.split_once(':')?;
        if name.is_empty() || !is_id(id) {
            return None;
        }
        out.push(':');
        escape_into(name, out);
        out.push(':');
    } else if let Some(stamp) = token.strip_prefix("t:") {
        let seconds = stamp.split(':').next()?;
        let at = OffsetDateTime::from_unix_timestamp(seconds.parse().ok()?).ok()?;
        let iso = at
            .format(&time::format_description::well_known::Rfc3339)
            .ok()?;
        let shown = at
            .format(time::macros::format_description!(
                "[year]-[month]-[day] [hour]:[minute] UTC"
            ))
            .ok()?;
        out.push_str(&format!("<time datetime=\"{iso}\">{shown}</time>"));
    } else if token.starts_with("https://") || token.starts_with("http://") {
        link(token, |out| escape_into(token, out), out);
    } else {
        return None;
    }
    Some(end + 1)
}

/// `[text](https://…)`.
fn masked_link(rest: &str, mentions: &Mentions, out: &mut String) -> Option<usize> {
    let close = rest.find("](")?;
    let text = &rest[1..close];
    let url_start = close + 2;
    let url_end = url_start + rest[url_start..].find(')')?;
    let url = &rest[url_start..url_end];
    if text.is_empty()
        || text.contains('\n')
        || !(url.starts_with("https://") || url.starts_with("http://"))
        || url.contains(char::is_whitespace)
    {
        return None;
    }
    link(url, |out| inline(text, mentions, out), out);
    Some(url_end + 1)
}

/// A bare URL, up to whitespace, less trailing punctuation that ends the sentence rather than the
/// link (and a closing parenthesis it did not open).
fn autolink(rest: &str, out: &mut String) -> usize {
    let mut end = rest
        .find(|c: char| c.is_whitespace() || c == '<')
        .unwrap_or(rest.len());
    loop {
        let url = &rest[..end];
        let trailing = url.ends_with(['.', ',', ':', ';', '!', '?', '"', '\''])
            || (url.ends_with(')') && url.matches('(').count() < url.matches(')').count());
        if !trailing {
            break;
        }
        end -= 1;
    }
    let url = &rest[..end];
    link(url, |out| escape_into(url, out), out);
    end
}

fn link(url: &str, text: impl FnOnce(&mut String), out: &mut String) {
    out.push_str("<a href=\"");
    escape_into(url, out);
    out.push_str("\" target=\"_blank\" rel=\"noopener noreferrer nofollow\">");
    text(out);
    out.push_str("</a>");
}

/// Bold, underline, strikethrough, spoilers, and italics, when the delimiter at `i` has a match.
fn emphasis(text: &str, i: usize, mentions: &Mentions, out: &mut String) -> Option<usize> {
    const DELIMITERS: [(&str, &str, &str); 6] = [
        ("**", "<strong>", "</strong>"),
        ("__", "<u>", "</u>"),
        ("~~", "<s>", "</s>"),
        ("||", "<span class=\"spoiler\">", "</span>"),
        ("*", "<em>", "</em>"),
        ("_", "<em>", "</em>"),
    ];
    let rest = &text[i..];
    let (delimiter, open, close) = DELIMITERS
        .into_iter()
        .find(|(delimiter, _, _)| rest.starts_with(delimiter))?;
    let single = delimiter.len() == 1;
    let word_char = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    // `_` italicises whole words only, so snake_case stays as it is.
    if delimiter == "_" && word_char(text[..i].chars().next_back()) {
        return None;
    }
    let start = delimiter.len();
    let mut search = start;
    let end = loop {
        let mut found = search + rest[search..].find(delimiter)?;
        // A doubled one closes on the end of a longer run (`***x***` is bold around italic).
        let run = rest[found..]
            .bytes()
            .take_while(|&b| b == delimiter.as_bytes()[0])
            .count();
        if !single && run > delimiter.len() {
            found += run - delimiter.len();
        }
        let next = rest[found + delimiter.len()..].chars().next();
        // A lone `*` or `_` does not close on half of a doubled one.
        let doubled = single
            && (next == delimiter.chars().next()
                || rest[..found].ends_with(delimiter) && found > start);
        let closes_word = delimiter != "_" || !word_char(next);
        if found > start && !doubled && closes_word {
            break found;
        }
        search = found + delimiter.len();
    };
    let inner = &rest[start..end];
    if single && (inner.starts_with(char::is_whitespace) || inner.ends_with(char::is_whitespace)) {
        return None;
    }
    out.push_str(open);
    inline(inner, mentions, out);
    out.push_str(close);
    Some(end + delimiter.len())
}

fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(source: &str) -> String {
        render(source, &Mentions::default())
    }

    #[test]
    fn escapes_everything_it_does_not_render() {
        assert_eq!(
            html("<script>alert(\"x\")</script> & 'y'"),
            "<p>&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt; &amp; &#39;y&#39;</p>"
        );
    }

    #[test]
    fn renders_discords_emphasis() {
        assert_eq!(
            html("**bold** __under__ ~~gone~~ ||secret|| *it* _also_"),
            "<p><strong>bold</strong> <u>under</u> <s>gone</s> \
             <span class=\"spoiler\">secret</span> <em>it</em> <em>also</em></p>"
        );
        assert_eq!(html("***both***"), "<p><strong><em>both</em></strong></p>");
        assert_eq!(
            html("snake_case_name stays"),
            "<p>snake_case_name stays</p>"
        );
        assert_eq!(html("2 * 3 * 4"), "<p>2 * 3 * 4</p>");
        assert_eq!(html("\\*not italic\\*"), "<p>*not italic*</p>");
        assert_eq!(html("**unclosed"), "<p>**unclosed</p>");
    }

    #[test]
    fn keeps_code_as_written() {
        assert_eq!(
            html("use `**x**` here"),
            "<p>use <code>**x**</code> here</p>"
        );
        assert_eq!(
            html("before\n```rust\nlet x = 1 < 2;\n```\nafter"),
            "<p>before</p><pre><code>let x = 1 &lt; 2;</code></pre><p>after</p>"
        );
    }

    #[test]
    fn lays_out_lines_and_blocks() {
        assert_eq!(html("one\ntwo\n\nthree"), "<p>one<br>two</p><p>three</p>");
        assert_eq!(
            html("# Raid night\n-# small print"),
            "<h3>Raid night</h3><p><small>small print</small></p>"
        );
        assert_eq!(
            html("- a\n- b\n2. c\n3. d"),
            "<ul><li>a</li><li>b</li></ul><ol start=\"2\"><li>c</li><li>d</li></ol>"
        );
        assert_eq!(
            html("> quoted\n> more\nnot"),
            "<blockquote><p>quoted<br>more</p></blockquote><p>not</p>"
        );
        assert_eq!(
            html(">>> all\nof this"),
            "<blockquote><p>all<br>of this</p></blockquote>"
        );
    }

    #[test]
    fn links_only_web_urls() {
        let a = |url: &str, text: &str| {
            format!(
                "<a href=\"{url}\" target=\"_blank\" rel=\"noopener noreferrer nofollow\">{text}</a>"
            )
        };
        assert_eq!(
            html("[the plan](https://example.com/a?b=1&c=2)"),
            format!(
                "<p>{}</p>",
                a("https://example.com/a?b=1&amp;c=2", "the plan")
            )
        );
        assert_eq!(
            html("[x](javascript:alert(1))"),
            "<p>[x](javascript:alert(1))</p>"
        );
        assert_eq!(
            html("see https://example.com/x."),
            format!(
                "<p>see {}.</p>",
                a("https://example.com/x", "https://example.com/x")
            )
        );
        assert_eq!(
            html("<https://example.com>"),
            format!("<p>{}</p>", a("https://example.com", "https://example.com"))
        );
    }

    #[test]
    fn names_mentions_from_when_it_was_posted() {
        let mut mentions = Mentions::default();
        mentions.users.insert("1".into(), "Ey".into());
        mentions.roles.insert("2".into(), "Raider".into());
        mentions.channels.insert("3".into(), "raids".into());
        assert_eq!(
            render("<@1> <@!1> <@&2> <#3> <@9> <:pog:123> <t:0:F>", &mentions),
            "<p><span class=\"mention\">@Ey</span> <span class=\"mention\">@Ey</span> \
             <span class=\"mention\">@Raider</span> <span class=\"mention\">#raids</span> \
             <span class=\"mention\">@unknown-user</span> :pog: \
             <time datetime=\"1970-01-01T00:00:00Z\">1970-01-01 00:00 UTC</time></p>"
        );
        assert_eq!(
            render("<@1><b>", &mentions),
            "<p><span class=\"mention\">@Ey</span>&lt;b&gt;</p>"
        );
    }
}
