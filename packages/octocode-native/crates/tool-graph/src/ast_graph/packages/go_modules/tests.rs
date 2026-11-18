use super::*;

#[test]
fn directives_read_as_one_line_or_a_block() {
    let text = "module x.io/a\nreplace x.io/b => ../b\nreplace x.io/far v1.0.0 => example.com/fork v1.0.0\nreplace (\n\tx.io/c v1.2.3 => ./c\n\t// note\n\tx.io/d => example.com/d v1\n)\n";
    let (dir, root) = (Path::new("/r/svc"), Path::new("/r/svc"));
    assert_eq!(
        local_replacements(text, dir, root),
        [("x.io/b".to_owned(), true), ("x.io/c".to_owned(), false)]
    );
    assert_eq!(
        directive_entries("go 1.22\nuse ./a\nuse (\n\t./b\n)\nuseless\n", "use"),
        ["./a", "./b"]
    );
}
