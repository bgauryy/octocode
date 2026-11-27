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
    pub(super) async fn empty_type_resolution(&self) -> Result<EmptyType, LspFailure> {
        if self.language_id != Some("rust") {
            return Ok(EmptyType::default());
        }
        let hover = cancellable(
            self.cancel,
            self.client
                .get_hover(self.path.to_owned(), self.line, self.character),
        )
        .await?
        .unwrap_or(Value::Null);
        let named = !hover.is_null() && !hover.to_string().contains(UNRESOLVED_TYPE);
        let definition = match self.location_request(LocationRequest::Definition).await {
            Ok(found) => found,
            Err(_) => {
                self.cancel.check().map_err(LspFailure::cancelled)?;
                Vec::new()
            }
        };
        // The anchor is the type's own declaration: rust-analyzer gives a
        // declaration no type location, and `definition` is that declaration.
        let declares = lsp_search_query::symbol_name(self.query)
            .is_some_and(|name| hover_declares_type(&hover, name));
        if named && declares && !definition.is_empty() {
            return Ok(EmptyType {
                resolved: Some(AnchorResolved::Name),
                declaration: definition,
            });
        }
        let resolved = if named {
            Some(AnchorResolved::Type)
        } else if !definition.is_empty() {
            Some(AnchorResolved::Name)
        } else {
            None
        };
        Ok(EmptyType {
            resolved,
            declaration: Vec::new(),
        })
    }
}

/// What an empty `typeDefinition` answer resolved to.
#[derive(Default)]
pub(super) struct EmptyType {
    pub(super) resolved: Option<AnchorResolved>,
    /// The anchor's own type declaration, when the anchor is one.
    pub(super) declaration: Vec<octocode_engine::lsp::types::LspCodeSnippet>,
}

/// Rust item keywords that declare a type.
const TYPE_KEYWORDS: [&str; 5] = ["struct", "enum", "union", "trait", "type"];

/// Whether `hover` shows the declaration of the type `name` (`struct Name`,
/// `enum Name<T>`, ...), as opposed to a value or parameter of that type.
fn hover_declares_type(hover: &Value, name: &str) -> bool {
    let text = hover.to_string();
    let words: Vec<&str> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect();
    words
        .windows(2)
        .any(|pair| TYPE_KEYWORDS.contains(&pair[0]) && pair[1] == name)
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
