use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContentsEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ContentsListing {
    pub entries: Vec<ContentsEntry>,
    pub raw_entry_count: usize,
}

impl<R: CredentialResolver> GitHubTransport<R> {
    pub async fn repository_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<ContentsListing, ProviderError> {
        let mut segments = vec!["repos", owner, repo, "contents"];
        if !path.is_empty() && path != "." {
            segments.push(path);
        }
        let mut url = self.endpoint().rest(&segments)?;
        url.query_pairs_mut().append_pair("ref", reference);
        let response = self.execute(RequestSpec::get(url), context).await?;
        let value: serde_json::Value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub repository contents response",
            )
        })?;
        let raw_entry_count = value.as_array().map_or(1, Vec::len);
        let raw_entries = match value {
            serde_json::Value::Array(entries) => entries,
            entry @ serde_json::Value::Object(_) => vec![entry],
            _ => {
                return Err(ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub repository contents response",
                ));
            }
        };
        let entries = raw_entries
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "invalid GitHub repository contents entry",
                )
            })?;
        Ok(ContentsListing {
            entries,
            raw_entry_count,
        })
    }
}
