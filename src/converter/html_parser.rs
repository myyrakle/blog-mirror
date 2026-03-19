use scraper::{ElementRef, Html, Selector};

/// Converts a Naver blog post HTML string into Markdown.
/// Supports both Naver Smart Editor 3 (`se-main-container`) and legacy editor (`postViewArea`).
/// Also handles the case where the input is the inner HTML of `se-main-container` (stored in DB).
pub fn convert_html_to_markdown(html: &str) -> String {
    let document = Html::parse_document(html);

    let content_sel = Selector::parse(".se-main-container").unwrap();
    let legacy_sel = Selector::parse("#postViewArea").unwrap();
    let post_view_sel = Selector::parse("._postView").unwrap();
    let post_ct_sel = Selector::parse(".post_ct").unwrap();
    let body_sel = Selector::parse("body").unwrap();

    // Try selectors in priority order:
    // 1. SE3 main container
    // 2. Legacy postViewArea
    // 3. SE2 .post_ct (actual content) — BEFORE ._postView which is the outer wrapper
    // 4. SE2 ._postView (outer wrapper, fallback if .post_ct not present)
    // 5. body — covers the case where we stored inner_html directly
    let root = document
        .select(&content_sel)
        .next()
        .or_else(|| document.select(&legacy_sel).next())
        .or_else(|| document.select(&post_ct_sel).next())
        .or_else(|| document.select(&post_view_sel).next())
        .or_else(|| document.select(&body_sel).next());

    if let Some(root) = root {
        convert_element(root)
    } else {
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
    let normalized = normalize_blank_lines(&out);
    normalized.trim().to_string()
}

fn walk_element(el: ElementRef, out: &mut String, list_depth: usize) {
    use scraper::node::Node;

    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.text.as_ref();
                // Filter zero-width spaces (Naver editor artifact)
                let filtered: String = t.chars().filter(|&c| c != '\u{200B}').collect();
                if !filtered.trim().is_empty() {
                    out.push_str(&filtered);
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

    let has_class = |name: &str| classes.contains(&name);

    // --- Skip non-content tags ---
    if tag == "script" || tag == "style" || tag == "noscript" {
        return;
    }

    // --- Skip OG link preview widgets ---
    if has_class("se-oglink") {
        return;
    }

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
    // SE3: .se-component.se-code contains .__se_code_view.language-XXX
    if has_class("se-code") || tag == "pre" {
        let code_view_sel = Selector::parse(".__se_code_view").unwrap();
        if let Some(code_view) = el.select(&code_view_sel).next() {
            let lang = code_view
                .value()
                .attr("class")
                .unwrap_or("")
                .split_whitespace()
                .find(|c| c.starts_with("language-"))
                .map(|c| c.trim_start_matches("language-"))
                .unwrap_or("");
            out.push_str(&format!("\n```{}\n", lang));
            out.push_str(code_view.text().collect::<String>().trim_end());
            out.push_str("\n```\n");
            return;
        }
        // Legacy: <pre><code class="language-xxx">
        out.push_str("\n```\n");
        let code_sel = Selector::parse("code").unwrap();
        if let Some(code_el) = el.select(&code_sel).next() {
            let lang = code_el
                .value()
                .attr("class")
                .unwrap_or("")
                .split_whitespace()
                .find(|c| c.starts_with("language-"))
                .map(|c| c.trim_start_matches("language-"))
                .unwrap_or("");
            if !lang.is_empty() {
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

    if tag == "code"
        && el
            .parent()
            .is_none_or(|p| ElementRef::wrap(p).is_none_or(|e| e.value().name() != "pre"))
    {
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
        // Naver lazy-loads images: src = low-res placeholder, data-lazy-src = actual image
        let src = el
            .value()
            .attr("data-lazy-src")
            .or_else(|| el.value().attr("src"))
            .unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("image");
        if !src.is_empty() {
            out.push_str(&format!("\n![{}]({})\n", alt, src));
        }
        return;
    }

    // --- Anchor ---
    if tag == "a" {
        let href = el.value().attr("href").unwrap_or("#");

        // Skip fragment-only links (Naver internal navigation: #ct, #, etc.)
        // Output just the text content without creating a link.
        if href == "#" || href.starts_with('#') {
            walk_element(el, out, list_depth);
            return;
        }

        // If anchor wraps an image, output the image instead of a link
        let img_sel = Selector::parse("img").unwrap();
        if let Some(img) = el.select(&img_sel).next() {
            let src = img
                .value()
                .attr("data-lazy-src")
                .or_else(|| img.value().attr("src"))
                .unwrap_or("");
            let alt = img.value().attr("alt").unwrap_or("image");
            if !src.is_empty() {
                out.push_str(&format!("\n![{}]({})\n", alt, src));
            }
            return;
        }
        let text = el.text().collect::<String>();
        let text = text.trim();
        if !text.is_empty() {
            out.push_str(&format!("[{}]({})", text, href));
        }
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

    // --- Paragraph / text blocks ---
    // Only match actual paragraph elements, not structural SE wrappers.
    // se-section, se-module, se-component-content are structural divs — just recurse.
    if tag == "p" || has_class("se-text-paragraph") {
        // Detect Naver large-font styled headings (se-fs-fs24 etc.)
        if let Some(prefix) = detect_naver_heading(el) {
            let mut text = String::new();
            push_plain_text(el, &mut text);
            let text = text.trim().to_string();
            if !text.is_empty() {
                out.push('\n');
                out.push_str(prefix);
                out.push_str(&text);
                out.push('\n');
            }
            return;
        }

        let inner = {
            let mut buf = String::new();
            walk_element(el, &mut buf, list_depth);
            buf
        };
        let trimmed = inner.trim();
        if !trimmed.is_empty() {
            // Use a hard line break (two trailing spaces) so consecutive non-empty
            // paragraphs appear as separate lines without extra paragraph spacing.
            // Naver's editor treats each Enter as a new <p>, not a new paragraph.
            out.push_str(trimmed);
            out.push_str("  \n");
        } else {
            // Empty paragraph (ZWS-only) = blank line / paragraph break in Naver.
            out.push('\n');
        }
        return;
    }

    // --- Line break ---
    // Two trailing spaces force a hard line break in Markdown.
    // A bare \n is treated as a space by most renderers.
    if tag == "br" {
        out.push_str("  \n");
        return;
    }

    // --- Default: recurse into children ---
    walk_element(el, out, list_depth);
}

/// Detects Naver SE3 large-font styled headings (se-fs-fsXX class on span).
/// Returns the markdown heading prefix if the paragraph should be a heading.
fn detect_naver_heading(el: ElementRef) -> Option<&'static str> {
    let span_sel = Selector::parse("span").unwrap();
    for span in el.select(&span_sel) {
        let classes = span.value().attr("class").unwrap_or("");
        for class in classes.split_whitespace() {
            if let Some(size_str) = class.strip_prefix("se-fs-fs") {
                if let Ok(size) = size_str.parse::<u32>() {
                    return match size {
                        24.. => Some("## "),
                        20..=23 => Some("### "),
                        18..=19 => Some("#### "),
                        _ => None,
                    };
                }
            }
        }
    }
    None
}

/// Collects plain text from an element, stripping all formatting markers.
/// Used for headings where bold/italic markers would look wrong (e.g. `## **text**`).
fn push_plain_text(el: ElementRef, out: &mut String) {
    use scraper::node::Node;
    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.text.as_ref();
                let filtered: String = t.chars().filter(|&c| c != '\u{200B}').collect();
                out.push_str(&filtered);
            }
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    push_plain_text(child_el, out);
                }
            }
            _ => {}
        }
    }
}

/// Collect inline text content of an element, applying inline formatting.
fn push_inline_text(el: ElementRef, out: &mut String) {
    use scraper::node::Node;

    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                let t = text.text.as_ref();
                // Filter zero-width spaces
                let filtered: String = t.chars().filter(|&c| c != '\u{200B}').collect();
                out.push_str(&filtered);
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

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);

    for (i, row) in rows.iter().enumerate() {
        out.push('|');
        for j in 0..col_count {
            let cell = row.get(j).map(|s| s.as_str()).unwrap_or("");
            out.push_str(&format!(" {} |", cell));
        }
        out.push('\n');

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
    let mut consecutive_blank = 0u32;

    for line in s.lines() {
        if line.trim().is_empty() {
            consecutive_blank += 1;
            if consecutive_blank == 1 {
                // First blank line: normal paragraph break
                result.push('\n');
            } else {
                // Additional blank lines: CommonMark collapses these, so use
                // a <br> HTML block to force visible line spacing.
                // The surrounding blank lines terminate the HTML block properly.
                result.push_str("<br>\n\n");
            }
        } else {
            consecutive_blank = 0;
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

    #[test]
    fn test_code_block_se3() {
        let html = r#"<div class="se-component se-code se-l-code_stripe"><div class="se-code-source"><div class="__se_code_view language-sql">SELECT 1;</div></div></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(md.contains("```sql"));
        assert!(md.contains("SELECT 1;"));
        assert!(md.contains("```"));
    }

    #[test]
    fn test_oglink_skipped() {
        let html = r#"<div class="se-component se-oglink se-l-text"><a class="se-oglink-info" href="https://example.com"><strong class="se-oglink-title">Title</strong><p class="se-oglink-summary">Desc</p></a></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(!md.contains("Title"));
        assert!(!md.contains("Desc"));
    }

    #[test]
    fn test_inner_html_fallback() {
        // Simulates DB-stored inner HTML (no se-main-container wrapper)
        let html = r#"<div class="se-component se-text se-l-default"><div class="se-component-content"><div class="se-section se-section-text"><div class="se-module se-module-text"><p class="se-text-paragraph">Hello from body</p></div></div></div></div>"#;
        let md = convert_html_to_markdown(html);
        assert!(md.contains("Hello from body"));
    }
}
