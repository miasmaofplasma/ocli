use color_eyre::eyre::eyre;
use tracing::instrument;

use crate::{
    cli,
    context::Context,
    ftypes::Field,
    status::Status,
    vault::{
        self, edit,
        frontmatter::{self, inner_yaml, set_or_append_field},
        markdown,
    },
};

#[instrument(skip(context))]
pub fn run(context: &Context) -> color_eyre::Result<()> {
    let cli::Command::Status { value: status } = &context.cli().command else {
        return Err(eyre!("status command incorrectly called"));
    };

    let note_path = super::note_path(context)?;
    edit::edit_note(&note_path, |text| {
        let document = markdown::parse(text);
        let fm_span =
            document
                .frontmatter()
                .ok_or_else(|| vault::VaultError::FrontmatterNotFound {
                    path: note_path.display().to_string(),
                })?;
        let inner = inner_yaml(document.get(fm_span));
        // D16: `status` sets its Display form and syncs `done` —
        // `Complete` ⇔ `done: true`. Both are owned fields, appended (with
        // the D26 warning) when a hand-written note lacks them.
        let inner = set_or_append_field(inner, "status", &Field::Str(status.to_string()))?;
        let done = matches!(status, Status::Complete);
        let inner = set_or_append_field(&inner, "done", &Field::Bool(done))?;

        Ok(frontmatter::splice_inner(text, fm_span, &inner))
    })?;
    Ok(())
}