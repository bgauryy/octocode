struct Left {
    line: u32,
}
struct Right {
    line: u32,
}
enum Wrapped {
    Left(Left),
    Right(Right),
    Bare,
}

#[test]
fn binds_a_shared_field_from_either_form() {
    let line = |query: &Wrapped| -> Option<u32> {
        variant_field!(query, newtype [Wrapped::Left => Left, Wrapped::Right => Right] line => Some(*line), else => None)
    };
    assert_eq!(line(&Wrapped::Left(Left { line: 3 })), Some(3));
    assert_eq!(line(&Wrapped::Right(Right { line: 4 })), Some(4));
    assert_eq!(line(&Wrapped::Bare), None);
    let both = |query: &Wrapped| -> Option<u32> {
        variant_field!(query, each [Wrapped::Left] left => Some(left.line), else => None)
    };
    assert_eq!(both(&Wrapped::Left(Left { line: 5 })), Some(5));
    assert_eq!(both(&Wrapped::Bare), None);
    assert_eq!(super::non_empty(&[]), None);
    assert_eq!(super::non_empty(&["a".into()]), Some(vec!["a".into()]));
}
