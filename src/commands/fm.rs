use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    ftypes,
    vault::{
        self, edit,
        frontmatter::{self, set_field},
        markdown,
    },
};

#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<()> {
    let cli::Command::FrontMatter { key, value } = &context.cli().command else {
        return Err(eyre!("fm command incorrectly called"));
    };

    // D18: ignore-listed fields belong to other tooling — never written.
    if context
        .config()
        .frontmatter
        .ignore
        .iter()
        .any(|field| field == key)
    {
        return Err(eyre!(
            "frontmatter field {key:?} is on the ignore list and cannot be written"
        ));
    }

    // D18: a field with no declared type is `string` (the default).
    let key_type = context
        .config()
        .frontmatter
        .types
        .get(key)
        .cloned()
        .unwrap_or(ftypes::FieldType::Str);
    let field = ftypes::Field::parse(&key_type, value)?;
    let note_path = super::note_path(context)?;

    edit::edit_note(&note_path, |text| {
        let document = markdown::parse(text);
        let fm_span =
            document
                .frontmatter()
                .ok_or_else(|| vault::VaultError::FrontmatterNotFound {
                    path: note_path.display().to_string(),
                })?;
        let inner = frontmatter::inner_yaml(document.get(fm_span));
        let inner = set_field(inner, key, &field)?;
        // The closure returns the note's *full* new text (D11): untouched
        // bytes verbatim, the edited inner frontmatter spliced back in.
        Ok(frontmatter::splice_inner(text, fm_span, &inner))
    })?;
    Ok(())
}
