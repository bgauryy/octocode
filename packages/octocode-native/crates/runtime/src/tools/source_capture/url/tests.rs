use super::*;
use octocode_contract_types::ErrorCode;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

mod image;
mod redirects;
mod selection;
mod storage;

/// A canned reply: status, Location, Content-Type, body.
type Canned = (u16, Option<&'static str>, Option<&'static str>, Vec<u8>);

/// Replies by URL (fragment excluded); records every requested URL.
#[derive(Default)]
struct Fake {
    replies: HashMap<String, Canned>,
    requested: RefCell<Vec<String>>,
}

impl Fake {
    fn page(mut self, url: &str, content_type: &'static str, body: &str) -> Self {
        self.replies.insert(
            url.into(),
            (200, None, Some(content_type), body.as_bytes().to_vec()),
        );
        self
    }

    fn bytes(mut self, url: &str, content_type: Option<&'static str>, body: Vec<u8>) -> Self {
        self.replies
            .insert(url.into(), (200, None, content_type, body));
        self
    }

    fn status(mut self, url: &str, status: u16, location: Option<&'static str>) -> Self {
        self.replies
            .insert(url.into(), (status, location, None, Vec::new()));
        self
    }
}

impl Fetch for Fake {
    fn get(&self, url: &Url) -> Result<Reply, ClassificationError> {
        let mut bare = url.clone();
        bare.set_fragment(None);
        self.requested.borrow_mut().push(bare.to_string());
        let (status, location, content_type, body) = self
            .replies
            .get(bare.as_str())
            .cloned()
            .unwrap_or((404, None, None, Vec::new()));
        Ok(Reply {
            status,
            location: location.map(str::to_owned),
            content_type: content_type.map(str::to_owned),
            body,
        })
    }
}

const PAGE: &str = r#"<html><body><nav>Home</nav>
<h1>Guide</h1><p>Intro.</p>
<h2 id="rotation">Rotation</h2><p>Update the configuration, then restart.</p>
<h3>Check</h3><p>Run status.</p>
<h2 id="faq">FAQ</h2><p>None.</p></body></html>"#;

fn run(fake: &Fake, address: &str, max_chars: usize) -> Result<Selection, ClassificationError> {
    let security = ContentSecurity::new();
    let redaction = Redaction {
        security: &security,
        emails: false,
    };
    select(fake, parse(address)?, &redaction, &|| Ok(()), max_chars)
}
