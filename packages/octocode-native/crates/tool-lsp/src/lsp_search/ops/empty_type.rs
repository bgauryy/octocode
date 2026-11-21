//! An empty `typeDefinition` answer: what the server resolved at the anchor
//! decides whether unloaded source can explain it. A parameter of a
//! primitive type has no type location in any project, whatever a file the
//! package `include!`s from elsewhere happens to declare under its name.
use super::*;
use crate::lsp_search::scope::AnchorResolved;

/// How rust-analyzer writes a type it could not resolve, in hover text.
const UNRESOLVED_TYPE: &str = "{unknown}";

impl Operation<'_, '_> {
    /// What the server resolved at the anchor of an empty Rust type answer:
    /// its hover names the type ([`AnchorResolved::Type`]), or its
    /// `definition` places the name in loaded source
    /// ([`AnchorResolved::Name`]). `None` when it resolved neither, or for
    /// another language: the answer is then judged by the package's
    /// includes alone. A request the server fails resolves nothing.
    pub(super) async fn empty_type_resolution(&self) -> Result<Option<AnchorResolved>, LspFailure> {
        if self.language_id != Some("rust") {
            return Ok(None);
        }
        let hover = cancellable(
            self.cancel,
            self.client
                .get_hover(self.path.to_owned(), self.line, self.character),
        )
        .await?
        .unwrap_or(Value::Null);
        if !hover.is_null() && !hover.to_string().contains(UNRESOLVED_TYPE) {
            return Ok(Some(AnchorResolved::Type));
        }
        match self.location_request(LocationRequest::Definition).await {
            Ok(found) if !found.is_empty() => Ok(Some(AnchorResolved::Name)),
            Ok(_) => Ok(None),
            Err(_) => {
                self.cancel.check().map_err(LspFailure::cancelled)?;
                Ok(None)
            }
        }
    }
}

/// State on an empty type row that the type is known and has no location.
pub(super) fn state(row: &mut Value, resolved: AnchorResolved) {
    if resolved == AnchorResolved::Type
        && row.get("category").and_then(Value::as_str) == Some("noLocations")
    {
        row["reason"] = json!(
            "typeDefinitionProvider returned no locations: the server resolved the type at this anchor, and it has no declaration in source (a primitive or built-in type)."
        );
    }
}
