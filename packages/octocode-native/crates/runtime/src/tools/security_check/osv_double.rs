//! A recorded OSV for tests: what a walk of `/v1/querybatch` pages and
//! `/v1/vulns/{id}` records reads, written as the pages of whole records a
//! `/v1/query` walk once returned.
use crate::providers::artifact::test_http::RouteHttp;
use serde_json::{Value, json};

/// The `modified` stamp of a record that states none.
const MODIFIED: &str = "2024-01-01T00:00:00Z";

pub(crate) trait OsvPages: Sized {
    /// Answers the n-th `/v1/querybatch` request with the n-th `(status, page)`:
    /// a 200 page `{vulns: [records], next_page_token}` (or a whole batch answer, which has `results`) lists its records by id
    /// and `modified`, and each record answers `/v1/vulns/{id}`. Any other
    /// status answers with that status and body as given.
    fn osv_pages(self, pages: Vec<(u16, Value)>) -> Self;

    /// Every OSV query received, in order: the queries of each batch request.
    fn osv_queries(&self) -> Vec<Value>;
}

impl OsvPages for RouteHttp {
    fn osv_pages(self, pages: Vec<(u16, Value)>) -> Self {
        let mut http = self;
        let mut answers = Vec::new();
        for (status, page) in pages {
            if status != 200 {
                answers.push((status, page));
                continue;
            }
            if page.get("results").is_some() {
                answers.push((200, page));
                continue;
            }
            let mut result = json!({});
            if let Some(token) = page.get("next_page_token") {
                result["next_page_token"] = token.clone();
            }
            if let Some(records) = page.get("vulns").and_then(Value::as_array) {
                let listed = records
                    .iter()
                    .filter_map(|record| {
                        let id = record.get("id")?.as_str()?;
                        let modified = record.get("modified").and_then(Value::as_str);
                        Some((
                            id.to_owned(),
                            modified.unwrap_or(MODIFIED).to_owned(),
                            record,
                        ))
                    })
                    .collect::<Vec<_>>();
                result["vulns"] = listed
                    .iter()
                    .map(|(id, modified, _)| json!({"id": id, "modified": modified}))
                    .collect();
                for (id, _, record) in listed {
                    http = http.json(&format!("/v1/vulns/{id}"), record.clone());
                }
            }
            answers.push((200, json!({"results": [result]})));
        }
        http.post_sequence(answers)
    }

    fn osv_queries(&self) -> Vec<Value> {
        self.post_bodies()
            .iter()
            .flat_map(|body| body["queries"].as_array().cloned().unwrap_or_default())
            .collect()
    }
}
