//! Region locator for Obsidian note text (D27 leaf): finds the frontmatter
//! block, `#`-heading sections, and the ocli footer marker (D8) as byte
//! ranges over the *original* text.
//!
//! The file is a sequence of bytes; the parse is a labeled map of that same
//! text — nothing is copied, re-serialized, or dropped, so the D11
//! byte-preservation contract holds by construction.

/// The footer marker (D8): a literal line marking everything after it as
/// footer. ocli never writes past it; a note may omit it (no footer).
pub const FOOTER_MARKER: &str = "<!-- ocli:footer -->";
/// A byte range into [`Document`]'s original text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A `#`-heading and its content range: starts at the heading line itself
/// and ends where the next heading starts, at the footer marker, or at
/// end of text.
///
/// Names are bare (`"Progress"`, no `##`). What a name *means* — which
/// sections ocli owns — is the domain layer's business (D19/D27); this
/// module only reports what the text says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub content: Span,
}

/// In-progress section: opened at a heading, closed by the next heading
/// (or end of text). Never escapes `parse`.
struct TempSection {
    name: String,
    start: usize,
}

/// A parsed note: labeled ranges over the unchanged original text.
#[derive(Debug)]
pub struct Document<'a> {
    text: &'a str,
    frontmatter: Option<Span>,
    body: Span,
    sections: Vec<Section>,
    /// First footer-marker line to end of text (D8); `None` when the note
    /// has no marker.
    footer: Option<Span>,
}

/// Frontmatter only counts when the file's very first line is exactly `---`
/// (LF or CRLF). Anything before it — blank line, BOM, prose — means no
/// frontmatter; those bytes stay in the body and are preserved verbatim.
fn starts_with_delimiter(text: &str) -> bool {
    text.starts_with("---\n") || text.starts_with("---\r\n")
}

pub fn parse(text: &str) -> Document<'_> {
    // Scan state — all resolved into spans AFTER the loop, from geometry.
    let mut frontmatter_end: Option<usize> = None;
    let mut in_frontmatter = starts_with_delimiter(text);
    let mut body_start = 0;
    let mut in_fence = false;
    let mut current: Option<TempSection> = None;
    let mut sections: Vec<Section> = Vec::new();
    let mut footer_start: Option<usize> = None;
    let mut offset = 0;

    for (line_num, chunk) in text.split_inclusive('\n').enumerate() {
        // Line content without its terminator. Works uniformly for LF,
        // CRLF, and the unterminated final line.
        let line = chunk
            .strip_suffix('\n')
            .map_or(chunk, |l| l.strip_suffix('\r').unwrap_or(l));
        let eol = offset + chunk.len();

        if in_frontmatter {
            if line_num == 0 {
                // The opening delimiter: already verified by
                // `starts_with_delimiter`, so don't treat it as the closer.
                // Keep hunting.
            } else if line == "---" {
                // Closing delimiter: frontmatter spans both `---` lines,
                // since the write path must preserve them byte-for-byte.
                frontmatter_end = Some(eol);
                body_start = eol;
                in_frontmatter = false;
            }
            // Anything else is frontmatter content — keep scanning. If the
            // closer never arrives, `frontmatter_end` stays `None` and the
            // whole file is body (fail-soft, like Obsidian's hrule).
        } else {
            // Fence tracking: ``` or ~~~ toggles (Obsidian allows leading
            // whitespace). While inside a fence, `#` lines are content.
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_fence = !in_fence;
            }

            // Footer marker (D8): the first marker line outside any fence
            // ends the content region. Everything from it to EOF is footer —
            // ocli never writes past it, and headings inside the footer are
            // not content sections.
            if footer_start.is_none() && !in_fence && line.trim() == FOOTER_MARKER {
                // Close-on-successor: the open section ends at the marker.
                if let Some(prev) = current.take() {
                    sections.push(Section {
                        name: prev.name,
                        content: Span {
                            start: prev.start,
                            end: offset,
                        },
                    });
                }
                footer_start = Some(offset);
            }
            // Headings: 1–6 `#`, then space-or-end-of-line, flush-left
            // (CommonMark). `#hashtag` and `#######` are not headings.
            else if footer_start.is_none() && !in_fence && !line.starts_with(char::is_whitespace)
            {
                let hashes = line.chars().take_while(|&c| c == '#').count();
                let rest = &line[hashes..];
                let is_heading =
                    (1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(' '));

                if is_heading {
                    // Close-on-successor: the previous section ends where
                    // this heading starts — no overlap, no gap.
                    if let Some(prev) = current.take() {
                        sections.push(Section {
                            name: prev.name,
                            content: Span {
                                start: prev.start,
                                end: offset,
                            },
                        });
                    }
                    current = Some(TempSection {
                        name: rest.trim().to_string(),
                        start: offset,
                    });
                }
            }
        }

        offset = eol;
    }

    // Close the final section at end-of-text — it has no successor heading.
    // This also covers the unterminated-last-line case: with split_inclusive
    // it was just another chunk, and it may have opened this section.
    if let Some(prev) = current.take() {
        sections.push(Section {
            name: prev.name,
            content: Span {
                start: prev.start,
                end: text.len(),
            },
        });
    }

    Document {
        text,
        // Unclosed `---` → None, for free. No unwraps anywhere.
        frontmatter: frontmatter_end.map(|end| Span { start: 0, end }),
        // Tiling by construction: body starts where frontmatter ended (or
        // at 0) and runs to the end. Empty text → 0..0.
        body: Span {
            start: body_start,
            end: text.len(),
        },
        // First footer marker to EOF, or None (D8).
        footer: footer_start.map(|start| Span {
            start,
            end: text.len(),
        }),
        sections,
    }
}

impl<'a> Document<'a> {
    /// The original text, unchanged. Invariant: `render(parse(t)) == t`.
    pub fn render(&self) -> &'a str {
        self.text
    }

    /// The bytes of the original text covered by `span`.
    pub fn get(&self, span: Span) -> &'a str {
        &self.text[span.start..span.end]
    }

    /// The frontmatter block (both `---` lines included), if present.
    pub fn frontmatter(&self) -> Option<Span> {
        self.frontmatter
    }

    /// Everything after the frontmatter block (the whole text when there
    /// is none). Frontmatter and body tile the text exactly.
    pub fn body(&self) -> Span {
        self.body
    }
    /// The footer region (D8): from the first footer-marker line to the end
    /// of text, marker line included. `None` when the note has no marker —
    /// ocli then treats the whole body as content.
    pub fn footer(&self) -> Option<Span> {
        self.footer
    }

    /// `#`-heading sections in the content region (footer excluded), in
    /// document order.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frontmatter and body partition the text exactly, and every section
    /// lives inside the body.
    fn assert_tiling(doc: &Document, text: &str) {
        match doc.frontmatter() {
            Some(fm) => {
                assert_eq!(fm.start, 0);
                assert_eq!(doc.body().start, fm.end, "body starts at frontmatter end");
            }
            None => assert_eq!(doc.body().start, 0, "no frontmatter: body is whole file"),
        }
        assert_eq!(doc.body().end, text.len(), "body runs to end of text");

        let mut joined = String::new();
        if let Some(fm) = doc.frontmatter() {
            joined.push_str(doc.get(fm));
        }
        joined.push_str(doc.get(doc.body()));
        assert_eq!(joined, text, "frontmatter + body must reconstruct the text");

        for section in doc.sections() {
            assert!(
                section.content.start >= doc.body().start && section.content.end <= doc.body().end,
                "section {:?} must live inside the body",
                section.name
            );
        }
    }

    #[test]
    fn simple_note_frontmatter_and_body() {
        let text = "---\nstatus: In Progress\n---\n\nSome prose.\n";
        let doc = parse(text);
        assert_tiling(&doc, text);

        let fm = doc.frontmatter().expect("frontmatter present");
        assert_eq!(doc.get(fm), "---\nstatus: In Progress\n---\n");
        assert_eq!(doc.get(doc.body()), "\nSome prose.\n");
        assert!(doc.sections().is_empty());
    }

    #[test]
    fn no_frontmatter_when_file_starts_with_prose() {
        let text = "Just prose.\n## Notes\n- entry\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.frontmatter().is_none());
        assert_eq!(doc.get(doc.body()), text);
        assert_eq!(doc.sections().len(), 1);
        assert_eq!(doc.sections()[0].name, "Notes");
    }

    #[test]
    fn unclosed_frontmatter_is_none_and_body_is_whole_file() {
        let text = "---\nstatus: In Progress\nno closer ever\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.frontmatter().is_none());
        assert_eq!(doc.get(doc.body()), text);
    }

    #[test]
    fn delimiter_only_file_is_all_body() {
        let text = "---\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.frontmatter().is_none());
        assert_eq!(doc.get(doc.body()), text);
    }

    #[test]
    fn empty_frontmatter_block() {
        let text = "---\n---\nbody text\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        let fm = doc
            .frontmatter()
            .expect("empty frontmatter is still frontmatter");
        assert_eq!(doc.get(fm), "---\n---\n");
        assert_eq!(doc.get(doc.body()), "body text\n");
    }

    #[test]
    fn empty_text() {
        let doc = parse("");
        assert_tiling(&doc, "");
        assert!(doc.frontmatter().is_none());
        assert_eq!(doc.body(), Span { start: 0, end: 0 });
        assert!(doc.sections().is_empty());
    }

    #[test]
    fn file_without_trailing_newline() {
        let text = "---\nstatus: x\n---\nlast line";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert_eq!(doc.get(doc.body()), "last line");
    }

    #[test]
    fn adjacent_sections_end_exactly_at_next_heading() {
        let text = "## Progress\n- entry\n## Notes\n- other\n";
        let doc = parse(text);
        assert_tiling(&doc, text);

        let sections = doc.sections();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "Progress");
        assert_eq!(sections[1].name, "Notes");
        // No overlap, no gap: previous ends where next begins.
        assert_eq!(sections[0].content.end, sections[1].content.start);
        assert_eq!(doc.get(sections[1].content), "## Notes\n- other\n");
    }

    #[test]
    fn heading_as_unterminated_last_line() {
        let text = "---\nstatus: x\n---\n## Notes";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert_eq!(doc.sections().len(), 1);
        assert_eq!(doc.sections()[0].name, "Notes");
        assert_eq!(doc.get(doc.sections()[0].content), "## Notes");
    }

    #[test]
    fn crlf_note() {
        let text = "---\r\nstatus: x\r\n---\r\n## Notes\r\n- entry\r\n";
        let doc = parse(text);
        assert_tiling(&doc, text);

        let fm = doc.frontmatter().expect("CRLF frontmatter recognized");
        assert_eq!(doc.get(fm), "---\r\nstatus: x\r\n---\r\n");
        assert_eq!(doc.sections().len(), 1);
        assert_eq!(doc.sections()[0].name, "Notes", "no trailing \\r in name");
    }

    #[test]
    fn heading_inside_fence_is_not_a_section() {
        let text = "## Progress\n- entry\n```dataview\n# Not A Heading\n```\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert_eq!(doc.sections().len(), 1, "fenced # line must not split");
        assert_eq!(doc.sections()[0].name, "Progress");
        assert_eq!(
            doc.get(doc.sections()[0].content),
            "## Progress\n- entry\n```dataview\n# Not A Heading\n```\n"
        );
    }

    #[test]
    fn hashtag_and_seven_hashes_are_not_headings() {
        let text = "#tag line\n####### seven hashes\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.sections().is_empty());
    }

    #[test]
    fn single_hash_is_a_heading() {
        let text = "# Relates to\nwidget block\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert_eq!(doc.sections().len(), 1);
        assert_eq!(doc.sections()[0].name, "Relates to");
    }

    #[test]
    fn footer_marker_clamps_sections_and_hides_footer_headings() {
        let text = concat!(
            "## Progress\n",
            "- did a thing\n",
            "<!-- ocli:footer -->\n",
            "# Relates to\n",
            "```meta-bind\n",
            "INPUT[listSuggester:relates-to]\n",
            "```\n",
        );
        let doc = parse(text);
        assert_tiling(&doc, text);

        let marker_start = text.find(FOOTER_MARKER).unwrap();
        let footer = doc.footer().expect("marker present");
        assert_eq!(
            footer,
            Span {
                start: marker_start,
                end: text.len()
            }
        );
        assert_eq!(
            doc.get(footer),
            "<!-- ocli:footer -->\n# Relates to\n```meta-bind\nINPUT[listSuggester:relates-to]\n```\n"
        );

        let sections = doc.sections();
        assert_eq!(
            sections.len(),
            1,
            "footer headings are not content sections"
        );
        assert_eq!(sections[0].name, "Progress");
        assert_eq!(
            sections[0].content.end, marker_start,
            "content clamps at the marker line"
        );
    }

    #[test]
    fn marker_inside_fence_is_not_a_footer() {
        let text = "## Notes\n```text\n<!-- ocli:footer -->\n```\n# After\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.footer().is_none(), "marker inside a fence is content");
        assert_eq!(doc.sections().len(), 2);
        assert_eq!(doc.sections()[1].name, "After");
    }

    #[test]
    fn first_marker_wins_and_later_ones_are_footer_text() {
        let text = "<!-- ocli:footer -->\n# Widgets\n<!-- ocli:footer -->\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert_eq!(doc.footer().expect("footer present").start, 0);
        assert!(
            doc.sections().is_empty(),
            "everything after the marker is footer"
        );
    }

    #[test]
    fn marker_may_be_indented_and_crlf_terminated() {
        let text = "## Progress\r\n- entry\r\n  <!-- ocli:footer -->  \r\n# Widgets\r\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(
            doc.footer().is_some(),
            "trimmed line matches, CRLF tolerated"
        );
        assert_eq!(doc.sections().len(), 1);
    }

    #[test]
    fn marker_inside_frontmatter_is_ignored() {
        let text = "---\n<!-- ocli:footer -->\n---\n## Notes\n";
        let doc = parse(text);
        assert_tiling(&doc, text);
        assert!(doc.footer().is_none());
        assert_eq!(doc.sections().len(), 1);
        assert_eq!(doc.sections()[0].name, "Notes");
    }
}
