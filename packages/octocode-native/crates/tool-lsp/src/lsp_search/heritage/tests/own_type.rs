//! Outside TS/JS, a file that declares its own type of the base's name holds
//! subtypes of that type, not of the anchor's.
use super::*;

#[test]
fn a_file_declaring_its_own_type_of_the_name_is_set_aside() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    let policy = octocode_runtime_base::test_support::workspace_policy(&root);
    let write = |name: &str, text: &str| {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, text).expect("write");
        path.to_string_lossy().into_owned()
    };
    let java = write(
        "other/Model.java",
        "interface Shape {}\nclass Square implements Shape {}\n",
    );
    let python = write(
        "other/model.py",
        "class Shape:\n    pass\n\n\nclass Square(Shape):\n    pass\n",
    );
    assert!(declares_own_type(&policy, &java, "Shape"));
    assert!(declares_own_type(&policy, &python, "Shape"));
    // A file that only names the type implements the one it imports.
    let user = write("user/Tri.java", "class Tri implements Shape {}\n");
    assert!(!declares_own_type(&policy, &user, "Shape"));
    // A member of that name is no type declaration.
    let member = write(
        "user/holder.py",
        "class Holder:\n    def Shape(self):\n        pass\n",
    );
    assert!(!declares_own_type(&policy, &member, "Shape"));
}
