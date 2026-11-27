//! Build script: emits the build identity, the digest of every non-test source
//! of this crate and the crates it links (`octocode-runtime-base` included),
//! the lockfile and the contract inputs ([`identity`]).

use std::error::Error;

#[path = "build/identity.rs"]
mod identity;
#[path = "build/inputs.rs"]
mod inputs;

fn main() -> Result<(), Box<dyn Error>> {
    identity::emit_build_identity()
}
