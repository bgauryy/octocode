//! Fixtures for the workspace's tests, in a crate only dev-dependencies name,
//! so no release graph carries them: the wiremock mounts and responders
//! (`mock_http`), the fixture TOML grammar (`conformance`), and JSON values
//! written as JSON text and parsed at run time (`json_fixture`).
pub mod conformance;
mod json;
pub mod mock_http;

pub use json::{commit_field, json_fixture};
