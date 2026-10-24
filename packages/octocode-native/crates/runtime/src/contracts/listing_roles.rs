//! Listing field roles (contract `x-listing`, core `listing()`): what each
//! field of a listing query does to the walk, so a listing tool derives its
//! filter lists and the walk a cursor continues from the contract.
include!(concat!(env!("OUT_DIR"), "/listing_roles.rs"));

impl ListingRole {
    /// The field keeps entries by what they are (name or metadata), not
    /// by where they sit.
    #[must_use]
    pub(crate) const fn selects(self) -> bool {
        matches!(self, Self::Name | Self::Metadata)
    }
}
