//! Language ids one language server answers together: a TypeScript server
//! serves TS and JS (with JSX), clangd serves the C family. Every layer that
//! groups language ids (readiness budgets, lexical scan globs, coverage
//! signals) reads these lists.

/// Language ids the TypeScript server answers.
pub(crate) const TYPESCRIPT_FAMILY: &[&str] = &[
    "typescript",
    "typescriptreact",
    "javascript",
    "javascriptreact",
];

/// Language ids clangd answers.
pub(crate) const C_FAMILY: &[&str] = &["c", "cpp", "cuda"];

const FAMILIES: [&[&str]; 2] = [TYPESCRIPT_FAMILY, C_FAMILY];

/// The family `language_id` belongs to; `None` for a language its server
/// answers alone.
#[must_use]
pub fn family_of(language_id: &str) -> Option<&'static [&'static str]> {
    FAMILIES
        .into_iter()
        .find(|family| family.contains(&language_id))
}

/// Whether the TypeScript server answers `language_id`.
#[must_use]
pub fn is_typescript_family(language_id: Option<&str>) -> bool {
    language_id.is_some_and(|id| TYPESCRIPT_FAMILY.contains(&id))
}

/// Whether clangd answers `language_id`.
#[must_use]
pub fn is_c_family(language_id: Option<&str>) -> bool {
    language_id.is_some_and(|id| C_FAMILY.contains(&id))
}

#[cfg(test)]
mod tests;
