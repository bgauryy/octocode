use octocode_contract_types::{GhGetFileContentQuery, LocalFetchQuery};
use octocode_github::ProviderError;

pub(super) fn local_query(query: &GhGetFileContentQuery) -> Result<LocalFetchQuery, ProviderError> {
    let mut row = serde_json::to_value(query).map_err(decode_display)?;
    if let Some(row) = row.as_object_mut() {
        for key in ["owner", "repo", "ref", "refresh"] {
            row.remove(key);
        }
    }
    serde_json::from_value(row).map_err(decode_display)
}
pub(super) fn decode_display(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(
        octocode_github::ProviderErrorKind::Decode,
        error.to_string(),
    )
}
