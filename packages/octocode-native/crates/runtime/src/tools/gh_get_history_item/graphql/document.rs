//! The GraphQL documents of the pull-request fast path: the first-page
//! read and each connection's continuation.
use super::GraphqlWants;

/// One paged PR connection: its response key, the field (with its alias)
/// it selects, its page-size variable, the node fields, and where the PR
/// counts its items (`total`, a pointer into the PR), when it does.
pub(super) struct Connection {
    pub(super) key: &'static str,
    pub(super) field: &'static str,
    pub(super) variable: &'static str,
    pub(super) nodes: &'static str,
    /// A connection with a count beside it is read by REST past its first
    /// GraphQL page: REST sizes every batch from the count and reads them
    /// in one concurrent wave, where the cursor walks one spaced GraphQL
    /// request at a time.
    pub(super) total: Option<&'static str>,
}

pub(super) const FILES: Connection = Connection {
    key: "files",
    field: "files",
    variable: "files",
    nodes: "path additions deletions changeType",
    total: Some("/changedFiles"),
};
pub(super) const DISCUSSION: Connection = Connection {
    key: "commentsConn",
    field: "commentsConn: comments",
    variable: "discussion",
    nodes: "databaseId author{ login __typename } body createdAt url",
    total: Some("/comments/totalCount"),
};
pub(super) const REVIEWS: Connection = Connection {
    key: "reviews",
    field: "reviews",
    variable: "reviews",
    nodes: "databaseId author{ login __typename } state body submittedAt commit{ oid }",
    total: None,
};
pub(super) const COMMITS: Connection = Connection {
    key: "commits",
    field: "commits",
    variable: "commits",
    nodes: "commit{ oid message messageHeadline authoredDate committedDate author{ name user{ login } } }",
    total: None,
};

/// Nodes per connection page: GitHub's maximum, the size of one REST batch.
pub(super) const CONNECTION_PAGE: u32 = 100;

impl Connection {
    /// The connection's selection; `after` binds `$after` (a continuation).
    pub(super) fn selection(&self, after: bool) -> String {
        let after = if after { ",after:$after" } else { "" };
        format!(
            "{}(first:${}{after}){{ pageInfo{{ hasNextPage endCursor }} nodes{{ {} }} }}",
            self.field, self.variable, self.nodes
        )
    }

    /// The document that reads this connection's next page.
    pub(super) fn continuation_document(&self) -> String {
        format!(
            "query($owner:String!,$repo:String!,$number:Int!,${}:Int!,$after:String!){{ repository(owner:$owner,name:$repo){{ pullRequest(number:$number){{ {} }} }} }}",
            self.variable,
            self.selection(true)
        )
    }
}

/// The pull-request document for the collections GraphQL serves, and the
/// page size variable each collection binds. The body is selected only when
/// the read shows it (`body`); a row without it leaves `body` unknown, not
/// empty.
pub(super) fn pull_request_document(wants: &GraphqlWants) -> (String, Vec<(&'static str, u32)>) {
    let mut selections: Vec<String> = [
        "number title url state isDraft author { login }",
        "labels(first:20){ pageInfo{ hasNextPage } nodes { name } }",
        "baseRefName baseRefOid headRefName headRefOid createdAt updatedAt closedAt mergedAt mergeCommit { oid }",
        "comments { totalCount } changedFiles additions deletions",
        // Totals only: an alias keeps the count apart from a paged
        // `commits(first:)` selection of the same connection.
        "commitsCount: commits { totalCount } reviewThreads { totalCount }",
    ]
    .map(str::to_owned)
    .into();
    if wants.body {
        selections.push("body".to_owned());
    }
    let mut variables = Vec::new();
    let mut header = String::from("query($owner:String!,$repo:String!,$number:Int!");
    for (wanted, connection) in [
        (wants.files, &FILES),
        (wants.discussion, &DISCUSSION),
        (wants.reviews, &REVIEWS),
        (wants.commits, &COMMITS),
    ] {
        if wanted {
            selections.push(connection.selection(false));
            variables.push((connection.variable, CONNECTION_PAGE));
            header.push_str(&format!(",${}:Int!", connection.variable));
        }
    }
    header.push(')');
    let document = format!(
        "{header}{{ repository(owner:$owner,name:$repo){{ pullRequest(number:$number){{ {} }} }} }}",
        selections.join(" ")
    );
    (document, variables)
}
