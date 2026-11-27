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
#[macro_export]
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

pub use crate::variant_field;

/// A query's list in the engine's optional form: `None` when empty.
pub fn non_empty(values: &[String]) -> Option<Vec<String>> {
    (!values.is_empty()).then(|| values.to_vec())
}

#[cfg(test)]
mod tests;
