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
/// Two variant forms: `struct [Enum::A, Enum::B]` for struct variants
/// (`Enum::A { field, .. }`), and `newtype [Enum::A => StructA, ...]` for
/// variants wrapping a struct (`Enum::A(StructA { field, .. })`).
macro_rules! variant_field {
    ($query:expr, struct [$($variant:path),+ $(,)?] $field:ident => $value:expr $(, else => $other:expr)?) => {
        match $query {
            $($variant { $field, .. } => $value,)+
            $(_ => $other,)?
        }
    };
    ($query:expr, newtype [$($variant:path => $shape:path),+ $(,)?] $field:ident => $value:expr $(, else => $other:expr)?) => {
        match $query {
            $($variant($shape { $field, .. }) => $value,)+
            $(_ => $other,)?
        }
    };
}

pub(crate) use variant_field;

#[cfg(test)]
mod tests {
    enum Query {
        Wide { name: String, depth: u32 },
        Narrow { name: String },
    }

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
        let wide = Query::Wide {
            name: "a".into(),
            depth: 2,
        };
        let narrow = Query::Narrow { name: "b".into() };
        let name = |query: &Query| -> String {
            variant_field!(query, struct [Query::Wide, Query::Narrow] name => name.clone())
        };
        let depth = |query: &Query| -> Option<u32> {
            variant_field!(query, struct [Query::Wide] depth => Some(*depth), else => None)
        };
        assert_eq!((name(&wide), name(&narrow)), ("a".into(), "b".into()));
        assert_eq!((depth(&wide), depth(&narrow)), (Some(2), None));
        let line = |query: &Wrapped| -> Option<u32> {
            variant_field!(query, newtype [Wrapped::Left => Left, Wrapped::Right => Right] line => Some(*line), else => None)
        };
        assert_eq!(line(&Wrapped::Left(Left { line: 3 })), Some(3));
        assert_eq!(line(&Wrapped::Right(Right { line: 4 })), Some(4));
        assert_eq!(line(&Wrapped::Bare), None);
    }
}
