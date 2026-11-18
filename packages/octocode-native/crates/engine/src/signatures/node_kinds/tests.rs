use super::kind_of;

/// The table answers what `Node::kind` answers: for every kind id of
/// every grammar, and for every node of parsed sources.
#[test]
fn kind_names_equal_node_kind() {
    let samples = [
        (
            "rs",
            "fn main() { let x = foo!(a, b); x.unwrap(); } /* c */ macro_rules! m { () => {} }",
        ),
        (
            "ts",
            "class A<T> { m(x: T): void { return `a${x}`; } } const f = async () => <div/>;",
        ),
        ("py", "def f(x): return [i for i in x if i]  # c"),
        ("go", "package p\nfunc f() { go g() }"),
        ("java", "class A { void m() { new B(); } }"),
        ("c", "int main(void) { return 0; }"),
    ];
    for entry in crate::signatures::languages::all_entries() {
        let language = &entry.language;
        let table = super::kind_table(language);
        for id in 0..language.node_kind_count() {
            let id = u16::try_from(id).expect("ids fit");
            assert_eq!(
                table[usize::from(id)],
                language.node_kind_for_id(id).unwrap_or_default(),
                "{} kind {id}",
                entry.name
            );
        }
    }
    for (ext, source) in samples {
        let entry = crate::signatures::languages::find_entry(ext).expect("language");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&entry.language).expect("language");
        // A broken source too: ERROR and MISSING nodes keep their names.
        for text in [source.to_owned(), format!("{source} }} ) $ @@ {{")] {
            let tree = parser.parse(&text, None).expect("parse");
            let mut cursor = tree.walk();
            let mut visited = 0;
            loop {
                let node = cursor.node();
                assert_eq!(kind_of(node), node.kind(), "{ext}");
                visited += 1;
                if cursor.goto_first_child() || cursor.goto_next_sibling() {
                    continue;
                }
                while cursor.goto_parent() {
                    if cursor.goto_next_sibling() {
                        break;
                    }
                }
                if cursor.node() == tree.root_node() {
                    break;
                }
            }
            assert!(visited > 10);
        }
    }
}
