//! The one accessor layer over generated contract enums. A tool's query is a
//! generated enum whose variants each repeat the shared fields, so its
//! variant-independent accessors bind a field from whichever variant the
//! query is with [`variant_field!`] instead of one hand-written `match` arm
//! per variant.

/// Binds `$field` from whichever listed variant `$query` is, each of which
/// declares the field under that name, and evaluates `$value` with it bound
/// (by reference when `$query` is a reference). `else => $other` answers for
/// the variants not listed; without it the list must be exhaustive.
///
/// Two variant forms: `newtype [Enum::A => StructA, ...]` for variants
/// wrapping a struct (`Enum::A(StructA { field, .. })`), and
/// `each [Enum::A, Enum::B]` binding `$field` to the wrapped value itself
/// (`Enum::A(field)`), for an accessor that reads several of its fields.
macro_rules! variant_field {
    ($query:expr, newtype [$($variant:path => $shape:path),+ $(,)?] $field:ident => $value:expr $(, else => $other:expr)?) => {
        match $query {
            $($variant($shape { $field, .. }) => $value,)+
            $(_ => $other,)?
        }
    };
    ($query:expr, each [$($variant:path),+ $(,)?] $field:ident => $value:expr $(, else => $other:expr)?) => {
        match $query {
            $($variant($field) => $value,)+
            $(_ => $other,)?
        }
    };
}

pub(crate) use variant_field;

/// A query's list in the engine's optional form: `None` when empty.
pub(crate) fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

#[cfg(test)]
mod tests {
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
}
