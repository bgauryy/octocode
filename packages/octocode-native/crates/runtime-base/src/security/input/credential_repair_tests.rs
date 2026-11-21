use super::credential_repair;

/// A URL's repair is to drop its credentials; a search term's is a
/// non-secret fragment.
#[test]
fn the_repair_fits_the_field_that_held_the_credential() {
    for field in ["source.url", "resources.docs.url", "registryUrl"] {
        assert!(
            credential_repair(field).contains("drop the credentials from the URL"),
            "{field}"
        );
    }
    for field in ["matchString[]", "keywords[]", "symbolName"] {
        assert!(
            credential_repair(field).contains("non-secret fragment"),
            "{field}"
        );
    }
    // D8 (round 13): prose is searched by nothing; its repair is the text
    // without the credential.
    for field in [
        "mainGoal",
        "reasoning",
        "instructions",
        "background",
        "criteria",
        "questions.q1",
    ] {
        let repair = credential_repair(field);
        assert!(
            repair.contains("remove the credential from the text"),
            "{field}"
        );
        assert!(!repair.contains("fragment"), "{field}");
    }
}
