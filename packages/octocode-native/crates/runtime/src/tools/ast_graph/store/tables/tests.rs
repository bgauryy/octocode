use super::*;

#[test]
fn package_names_follow_ecosystem_rules() {
    assert_eq!(
        package_name("@scope/pkg/sub", "typescript").as_deref(),
        Some("@scope/pkg")
    );
    assert_eq!(
        package_name("react-dom/client", "javascript").as_deref(),
        Some("react-dom")
    );
    assert_eq!(
        package_name("serde::Deserialize", "rust").as_deref(),
        Some("serde")
    );
    assert_eq!(package_name("os.path", "python").as_deref(), Some("os"));
    assert_eq!(
        package_name("github.com/a/b/c", "go").as_deref(),
        Some("github.com/a/b")
    );
    assert_eq!(package_name("fmt", "go").as_deref(), Some("fmt"));
    assert_eq!(
        package_name("java.util.List", "java").as_deref(),
        Some("java.util")
    );
    assert_eq!(package_name("./local", "typescript"), None);
}

#[test]
fn callee_split_handles_member_and_path_calls() {
    assert_eq!(split_callee("run"), ("", "run"));
    assert_eq!(split_callee("this.run"), ("this", "run"));
    assert_eq!(split_callee("Self::new"), ("Self", "new"));
    assert_eq!(split_callee("a.b.c"), ("a.b", "c"));
    assert_eq!(first_segment("utils::inner"), "utils");
}
