use chrono::Local;
use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    config::SectionFormat,
    context::Context,
    vault::{edit, features::read_file_to_str, markdown, template::house_format},
};

#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<Option<String>> {
    let cli::Command::Section { name, action } = &context.cli().command else {
        return Err(eyre!("section command incorrectly called"));
    };

    // The section vocabulary is entirely config-driven (D19): any `[sections]`
    // key resolves to its heading + format.
    let spec = context
        .config()
        .sections
        .get(name)
        .ok_or_else(|| eyre!("unknown section {name:?}; define it in [sections]"))?;
    let (level, title) = split_heading(&spec.heading)
        .ok_or_else(|| eyre!("section {name:?} has an invalid heading {:?}", spec.heading))?;
    let note_path = super::note_path(context)?;

    let section = match action {
        None => {
            let text = read_file_to_str(&note_path)?;
            let section = find_section(&text, level, title).ok_or_else(|| {
                eyre!(
                    "section {:?} not found in {}",
                    spec.heading,
                    note_path.display()
                )
            })?;
            Some(section.to_string())
        }
        Some(cli::SectionAction::Add { text }) => {
            if text.contains(['\n', '\r']) {
                return Err(eyre!("section entries are a single line"));
            }
            let entry = match spec.format {
                SectionFormat::Log => format!("- {} — {text}", house_format(&Local::now())),
                SectionFormat::List => format!("- [ ] {text}"),
            };
            edit::edit_note(&note_path, |current| {
                Ok(append_to_section(
                    current,
                    level,
                    title,
                    &spec.heading,
                    &entry,
                ))
            })?;
            None
        }
    };
    Ok(section)
}

/// Splits `## Title` into `(2, "Title")` — the CommonMark heading shape the
/// parser recognizes (1–6 `#`, then a space, non-empty title).
fn split_heading(heading: &str) -> Option<(usize, &str)> {
    let hashes = heading.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = heading.get(hashes..)?;
    let title = rest.strip_prefix(' ')?.trim();
    (!title.is_empty()).then_some((hashes, title))
}

/// The section's text (heading line through its content), matched on
/// heading level *and* bare name.
fn find_section<'a>(text: &'a str, level: usize, title: &str) -> Option<&'a str> {
    let document = markdown::parse(text);
    document
        .sections()
        .iter()
        .find(|s| s.level == level && s.name == title)
        .map(|s| &text[s.content])
}

/// Appends `entry` to the section at `level`/`title`, creating it (using
/// `heading_line` verbatim) at the footer marker (D8) or EOF. Pure
/// text-in/text-out — the D11 splice unit for a section entry.
fn append_to_section(
    text: &str,
    level: usize,
    title: &str,
    heading_line: &str,
    entry: &str,
) -> String {
    let document = markdown::parse(text);
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };

    if let Some(section) = document
        .sections()
        .iter()
        .find(|s| s.level == level && s.name == title)
    {
        let trimmed = text[section.content].trim_end_matches(['\n', '\r']);
        let mut out = String::with_capacity(text.len() + entry.len());
        out.push_str(&text[..section.content.start]);
        out.push_str(trimmed);
        out.push_str(eol);
        out.push_str(entry);
        out.push_str(eol);
        out.push_str(&text[section.content.end..]);
        return out;
    }

    let at = document.footer().map_or(text.len(), |f| f.start);
    let mut out = String::with_capacity(text.len() + entry.len() + heading_line.len() + 4);
    out.push_str(&text[..at]);
    if !out.is_empty() && !out.ends_with(eol) {
        out.push_str(eol);
    }
    out.push_str(heading_line);
    out.push_str(eol);
    out.push_str(eol);
    out.push_str(entry);
    out.push_str(eol);
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_configured_heading_level() {
        let text = "## Progress\n\n- h2\n### Progress\n\n- h3\n";
        assert_eq!(
            find_section(text, 2, "Progress"),
            Some("## Progress\n\n- h2\n")
        );
        assert_eq!(
            find_section(text, 3, "Progress"),
            Some("### Progress\n\n- h3\n")
        );
    }

    #[test]
    fn appends_after_the_last_entry() {
        let text = "## Progress\n\n- first\n## Notes\n";
        let out = append_to_section(text, 2, "Progress", "## Progress", "- second");
        assert_eq!(out, "## Progress\n\n- first\n- second\n## Notes\n");
    }

    #[test]
    fn creates_a_h3_section_before_the_footer() {
        let text = "body\n<!-- ocli:footer -->\nkept\n";
        let out = append_to_section(text, 3, "Todo", "### Todo", "- first");
        assert_eq!(
            out,
            "body\n### Todo\n\n- first\n<!-- ocli:footer -->\nkept\n"
        );
    }

    #[test]
    fn creates_at_eof_when_no_footer() {
        let text = "body\n";
        let out = append_to_section(text, 2, "Progress", "## Progress", "- first");
        assert_eq!(out, "body\n## Progress\n\n- first\n");
    }
}
