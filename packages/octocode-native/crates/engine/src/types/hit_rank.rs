//! Internal hit ordering algebra shared by search producers and stored hit types.

/// Highest lexical class; defines the width of each source class.
pub(crate) const DECLARATION_RANK: u32 = 3;

/// Every production class outranks every test class, retaining lexical identity.
pub(crate) fn source_rank(lexical: u32, production: bool) -> u32 {
    lexical_rank(lexical) + u32::from(production) * (DECLARATION_RANK + 1)
}

pub(crate) fn lexical_rank(rank: u32) -> u32 {
    rank % (DECLARATION_RANK + 1)
}

pub(crate) fn is_production_rank(rank: u32) -> bool {
    rank > DECLARATION_RANK
}
