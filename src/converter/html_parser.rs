use scraper::{ElementRef, Html, Selector};

/// Converts a Naver blog post HTML string into Markdown.
/// Supports both Naver Smart Editor 3 (`se-main-container`) and legacy editor (`postViewArea`).
pub fn convert_html_to_markdown(html: &str) -> String {
    let document = Html::parse_document(html);

    // Try SE3 first, then fall back to legacy
    let content_sel = Selector::parse(".se-main-container").unwrap();
    let legacy_sel = Selector::parse("#postViewArea").unwrap();

    if let Some(root) = document.select(&content_sel).next() {
        convert_element(root)
    } else if let Some(root) = document.select(&legacy_sel).next() {
        convert_element(root)
    } else {
        // Fallback: just extract all text
        document
            .root_element()
            .text()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn convert_element(element: ElementRef) -> String {
    let mut out = String::new();
    walk_element(element, &mut out, 0);
    // Normalize excessive blank lines
    let normalized = normalize_blank_lines(&out);
    normalized.trim().to_string()
}

fn walk_element(el: ElementRef, out: &mut String, list_depth: usize) {
    use scraper::node::Node;

    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.text.as_ref();
                if !t.trim().is_empty() {
                    out.push_str(t);
                }
            }
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    handle_element(child_el, out, list_depth);
                }
            }
            _ => {}
        }
    }
}

fn handle_element(el: ElementRef, out: &mut String, list_depth: usize) {
    let tag = el.value().name();
    let classes: Vec<&str> = el
        .value()
        .attr("class")
        .unwrap_or("")
        .split_whitespace()
        .collect();

    let has_class = |name: &str| classes.iter().any(|c| *c == name);

    // --- Headings ---
    for (i, prefix) in ["# ", "## ", "### ", "#### ", "##### ", "###### "]
        .iter()
        .enumerate()
    {
        let se_class = format!("se-heading{}", i + 1);
        if has_class(&se_class) || tag == format!("h{}", i + 1) {
            out.push('\n');
            out.push_str(prefix);
            push_inline_text(el, out);
            out.push('\n');
            return;
        }
    }

    // --- Code block ---
    if has_class("se-code") || tag == "pre" {
        out.push_str("\n```\n");
        // Try to detect language from inner elements
        let code_sel = Selector::parse("code").unwrap();
        if let Some(code_el) = el.select(&code_sel).next() {
            let lang = code_el.value().attr("class").unwrap_or("");
            let lang = lang
                .split_whitespace()
                .find(|c| c.starts_with("language-"))
                .map(|c| c.trim_start_matches("language-"))
                .unwrap_or("");
            if !lang.is_empty() {
                // Replace the opening ``` with ```lang
                let new_fence = format!("```{}\n", lang);
                let len = out.len();
                out.truncate(len - 4); // remove "\n```\n"
                out.push('\n');
                out.push_str(&new_fence);
            }
            out.push_str(&code_el.text().collect::<String>());
        } else {
            out.push_str(&el.text().collect::<String>());
        }
        out.push_str("\n```\n");
        return;
    }

    if tag == "code" && !el.parent().map_or(false, |p| ElementRef::wrap(p).map_or(false, |e| e.value().name() == "pre")) {
        out.push('`');
        out.push_str(&el.text().collect::<String>());
        out.push('`');
        return;
    }

    // --- Blockquote ---
    if has_class("se-quotation") || tag == "blockquote" {
        let inner = {
            let mut buf = String::new();
            walk_element(el, &mut buf, list_depth);
            buf
        };
        out.push('\n');
        for line in inner.trim().lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        return;
    }

    // --- Horizontal rule ---
    if tag == "hr" {
        out.push_str("\n---\n");
        return;
    }

    // --- Ordered list ---
    if tag == "ol" {
        out.push('\n');
        let item_sel = Selector::parse("li").unwrap();
        let mut idx = 1usize;
        for item in el.select(&item_sel) {
            // Only direct children
            if item.parent().map(|p| p.id()) != Some(el.id()) {
                continue;
            }
            let indent = "   ".repeat(list_depth);
            out.push_str(&format!("{}{}. ", indent, idx));
            push_inline_text(item, out);
            out.push('\n');
            idx += 1;
        }
        out.push('\n');
        return;
    }

    // --- Unordered list ---
    if tag == "ul" {
        out.push('\n');
        let item_sel = Selector::parse("li").unwrap();
        for item in el.select(&item_sel) {
            if item.parent().map(|p| p.id()) != Some(el.id()) {
                continue;
            }
            let indent = "   ".repeat(list_depth);
            out.push_str(&format!("{}- ", indent));
            push_inline_text(item, out);
            out.push('\n');
        }
        out.push('\n');
        return;
    }

    // --- Table ---
    if tag == "table" {
        convert_table(el, out);
        return;
    }

    // --- Image ---
    if tag == "img" {
        let src = el.value().attr("src").unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("image");
        if !src.is_empty() {
            out.push_str(&format!("\n![{}]({})\n", alt, src));
        }
        return;
    }

    // --- Anchor ---
    if tag == "a" {
        let href = el.value().attr("href").unwrap_or("#");
        let text = el.text().collect::<String>();
        out.push_str(&format!("[{}]({})", text, href));
        return;
    }

    // --- Inline formatting ---
    if tag == "strong" || tag == "b" {
        out.push_str("**");
        push_inline_text(el, out);
        out.push_str("**");
        return;
    }
    if tag == "em" || tag == "i" {
        out.push('*');
        push_inline_text(el, out);
        out.push('*');
        return;
    }
    if tag == "s" || tag == "del" {
        out.push_str("~~");
        push_inline_text(el, out);
        out.push_str("~~");
        return;
    }

    // --- Paragraph / block containers ---
    if tag == "p"
        || has_class("se-text-paragraph")
        || has_class("se-module")
        || has_class("se-section")
    {
        let inner = {
            let mut buf = String::new();
            walk_element(el, &mut buf, list_depth);
            buf
        };
        let trimmed = inner.trim();
        if !trimmed.is_empty() {
            out.push('\n');
            out.push_str(trimmed);
            out.push('\n');
        }
        return;
    }

    // --- Line break ---
    if tag == "br" {
        out.push('\n');
        return;
    }

    // --- Default: recurse ---
    walk_element(el, out, list_depth);
}

/// Collect inline text content of an element, applying inline formatting.
fn push_inline_text(el: ElementRef, out: &mut String) {
    use scraper::node::Node;

    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                out.push_str(text.text.as_ref());
            }
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    handle_element(child_el, out, 0);
                }
            }
            _ => {}
        }
    }
}

fn convert_table(el: ElementRef, out: &mut String) {
    let tr_sel = Selector::parse("tr").unwrap();
    let th_sel = Selector::parse("th").unwrap();
    let td_sel = Selector::parse("td").unwrap();

    out.push('\n');
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut is_header_row: Vec<bool> = Vec::new();

    for tr in el.select(&tr_sel) {
        let mut row = Vec::new();
        let mut has_th = false;
        for th in tr.select(&th_sel) {
            row.push(th.text().collect::<String>().trim().to_string());
            has_th = true;
        }
        for td in tr.select(&td_sel) {
            row.push(td.text().collect::<String>().trim().to_string());
        }
        if !row.is_empty() {
            is_header_row.push(has_th);
            rows.push(row);
        }
    }

    if rows.is_empty() {
        return;
    }

    // Determine column count
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);

    for (i, row) in rows.iter().enumerate() {
        out.push('|');
        for j in 0..col_count {
            let cell = row.get(j).map(|s| s.as_str()).unwrap_or("");
            out.push_str(&format!(" {} |", cell));
        }
        out.push('\n');

        // Insert separator after first header row
        if i == 0 && is_header_row.first().copied().unwrap_or(false) {
            out.push('|');
            for _ in 0..col_count {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out.push('\n');
}

fn normalize_blank_lines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut blank_count = 0usize;

    for line in s.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_paragraph() {
        let html = r#"<div class="se-main-container"><p class="se-text-paragraph">Hello World</p></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(md.contains("Hello World"));
    }

    #[test]
    fn test_heading() {
        let html = r#"<div class="se-main-container"><h2 class="se-heading2">My Title</h2></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(md.contains("## My Title"));
    }

    #[test]
    fn test_bold() {
        let html = r#"<div class="se-main-container"><p><strong>bold text</strong></p></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(md.contains("**bold text**"));
    }
}
