//! GitHub host integration: credential policy, OAuth, and relative query dates.
mod auth;
mod dates;
pub mod login;

pub use auth::{
    AuthMode, AuthSelection, Authentication, CredentialStore, OAuthToken, StoredCredentials,
    configured_credential_host,
};
pub use dates::utc_timestamp;
