use octocode_contract_types::LocalFetchQuery;
use octocode_github::{ProviderError, ProviderErrorKind};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(super) struct Matcher {
    query: LocalFetchQuery,
    failure: Mutex<Option<String>>,
}
impl Matcher {
    pub(super) fn of(
        query: &octocode_contract_types::GhGetHistoryItemQuery,
    ) -> Result<Option<Arc<Self>>, serde_json::Error> {
        let Some(terms) = super::gh_get_history_item_query::match_strings(query) else {
            return Ok(None);
        };
        let Some((case_mode, regex_engine, whole_word)) =
            super::gh_get_history_item_query::match_options(query)
        else {
            return Ok(None);
        };
        let local = serde_json::from_value(
            serde_json::json!({"path":"history.patch","matchString":terms,"caseMode":case_mode,"regexEngine":regex_engine,"wholeWord":whole_word}),
        )?;
        crate::tools::source_view::match_spans(&local, "")
            .map_err(|e| <serde_json::Error as serde::de::Error>::custom(e.detail))?;
        Ok(Some(Arc::new(Self {
            query: local,
            failure: Mutex::new(None),
        })))
    }
    pub(super) fn matches(&self, text: &str) -> bool {
        match crate::tools::source_view::match_spans(&self.query, text) {
            Ok(found) if !found.truncated => !found.spans.is_empty(),
            result => {
                let message = match result {
                    Err(error) => error.detail,
                    Ok(_) => "History matching reached the regex match limit; this result is incomplete. Narrow the file scope.".to_owned(),
                };
                *self
                    .failure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(message);
                false
            }
        }
    }
    pub(super) fn result(&self) -> Result<(), ProviderError> {
        match &*self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            Some(message) => Err(ProviderError::new(
                ProviderErrorKind::Validation,
                message.clone(),
            )),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn history_uses_the_shared_matcher_for_every_match_variant() {
        for identity in [
            json!({"operation":"pullRequest","number":1}),
            json!({"operation":"commit","ref":"abc"}),
            json!({"operation":"compare","baseRef":"base","headRef":"head"}),
        ] {
            for (options, hit, miss) in [
                (json!({"matchString":["Token", "other"]}), "Token", "token"),
                (
                    json!({"matchString":["Token"],"caseMode":"insensitive"}),
                    "token",
                    "toke",
                ),
                (
                    json!({"matchString":["token"],"wholeWord":true}),
                    "a token b",
                    "tokenized",
                ),
                (
                    json!({"matchString":["tok.n"],"regexEngine":"literal"}),
                    "tok.n",
                    "token",
                ),
                (
                    json!({"matchString":["tok.n"],"regexEngine":"rust"}),
                    "token",
                    "tokn",
                ),
                (
                    json!({"matchString":["(?<=pre)token"],"regexEngine":"pcre2"}),
                    "pretoken",
                    "token",
                ),
            ] {
                let mut row = json!({"owner":"a","repo":"b"});
                for fields in [&identity, &options] {
                    for (key, value) in fields.as_object().expect("fields") {
                        row[key] = value.clone();
                    }
                }
                let typed = serde_json::from_value(row).expect("typed match variant");
                let matcher = Matcher::of(&typed)
                    .expect("valid matcher")
                    .expect("match supplied");
                assert!(
                    matcher.matches(hit),
                    "{identity}: {options} must match {hit}"
                );
                assert!(
                    !matcher.matches(miss),
                    "{identity}: {options} must reject {miss}"
                );
                matcher.result().expect("complete match evaluation");
            }
        }
    }
}
