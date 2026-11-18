use super::*;

#[test]
fn the_anchor_is_the_running_executable() {
    let exe = std::fs::canonicalize(std::env::current_exe().expect("exe")).expect("canonical");
    assert_eq!(install_anchor(), Some(exe));
}
