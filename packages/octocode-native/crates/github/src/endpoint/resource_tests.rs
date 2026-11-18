use super::GitHubEndpoint;
use crate::budget::GitHubResource;
use url::Url;

fn endpoint(base: &str) -> GitHubEndpoint {
    GitHubEndpoint::new(Url::parse(base).expect("base")).expect("endpoint")
}

fn resource(endpoint: &GitHubEndpoint, path: &str) -> GitHubResource {
    let mut url = endpoint.rest(&[]).expect("root");
    url.set_path(path);
    endpoint.resource(&url)
}

#[test]
fn requests_are_classified_by_their_route_under_the_api_root() {
    use GitHubResource::{CodeSearch, Core, Graphql, Search};
    let dotcom = endpoint("https://api.github.com");
    let ghes = endpoint("https://ghe.example.com/api/v3");
    for (endpoint, root) in [(&dotcom, ""), (&ghes, "/api/v3")] {
        let cases = [
            ("/search/code", CodeSearch),
            ("/search/issues", Search),
            ("/search/repositories", Search),
            ("/search", Core),
            ("/search/", Core),
            ("/repos/a/b/issues", Core),
            // Owner, repository and ref names never pick the bucket.
            ("/repos/graphql/graphql-spec/contents/README.md", Core),
            ("/repos/apollographql/graphql-tools/commits/main", Core),
            ("/repos/search/code/contents/x", Core),
            ("/repos/owner/search/git/trees/HEAD", Core),
            ("/users/graphql", Core),
            ("/repos/a/b/commits/search/issues", Core),
        ];
        for (route, expected) in cases {
            assert_eq!(
                resource(endpoint, &format!("{root}{route}")),
                expected,
                "{root}{route}"
            );
        }
        assert_eq!(endpoint.resource(&endpoint.graphql()), Graphql);
    }
    // GHES serves GraphQL beside, not under, the REST root.
    assert_eq!(resource(&ghes, "/api/graphql"), Graphql);
    assert_eq!(resource(&ghes, "/api/v3/graphql"), Core);
    assert_eq!(resource(&dotcom, "/graphql"), Graphql);
}
