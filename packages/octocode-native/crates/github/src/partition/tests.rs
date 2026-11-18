use super::Partition;
use secrecy::SecretString;

fn partition(origin: &str, secret: Option<&str>) -> Partition {
    Partition::new(origin, secret.map(SecretString::from).as_ref())
}

#[test]
fn origin_and_exact_credential_separate_partitions() {
    let a = partition("https://api.github.com/", Some("one"));
    assert_eq!(a, partition("https://api.github.com/", Some("one")));
    assert_ne!(a, partition("https://api.github.com/", Some("two")));
    assert_ne!(a, partition("https://ghe.example/api/v3/", Some("one")));
    assert_ne!(
        a.identity(),
        partition("https://api.github.com/", None).identity()
    );
}

#[test]
fn empty_secret_is_anonymous() {
    let anonymous = partition("registry.npmjs.org", None);
    assert_eq!(anonymous.credential_tag(), "anon");
    assert_eq!(anonymous, partition("registry.npmjs.org", Some("")));
}

/// The tag is the persisted limiter label: the first 8 digest bytes.
#[test]
fn credential_tag_is_the_short_digest_and_debug_hides_the_secret() {
    let key = partition("github.com", Some("private-token"));
    assert_eq!(key.credential_tag().len(), 16);
    assert!(!format!("{key:?}").contains("private-token"));
    assert!(!key.identity().contains("private-token"));
}
