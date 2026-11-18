//! Tool contract embedded from `@octocodeai/config` (`contract/`) by build.rs.
#![expect(
    unreachable_pub,
    reason = "the generator writes `pub` items; this module is `pub(crate)`, so none is reachable from another crate"
)]
include!(concat!(env!("OUT_DIR"), "/tool_contract.rs"));
