use super::RefLists;
use octocode_test_support::json_fixture;

/// A request names the lists it wants; one that names none gets both.
#[test]
fn a_refs_read_lists_what_the_request_names() {
    let both = RefLists {
        tags: true,
        branches: true,
    };
    assert_eq!(
        RefLists::of(&json_fixture(r#"{"operation": "refs"}"#)),
        both
    );
    assert_eq!(RefLists::of(&json_fixture(r#"{"refKinds": []}"#)), both);
    assert_eq!(
        RefLists::of(&json_fixture(r#"{"refKinds": ["tags", "branches"]}"#)),
        both
    );
    assert_eq!(
        RefLists::of(&json_fixture(r#"{"refKinds": ["tags"]}"#)),
        RefLists {
            tags: true,
            branches: false
        }
    );
    assert_eq!(
        RefLists::of(&json_fixture(r#"{"refKinds": ["branches"]}"#)),
        RefLists {
            tags: false,
            branches: true
        }
    );
}
