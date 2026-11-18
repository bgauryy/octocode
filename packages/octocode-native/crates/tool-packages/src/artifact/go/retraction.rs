//! Module proxy reads and release retraction checks.
use super::{
    ArtifactError, ArtifactItem, ArtifactType, ErrorCode, JSON, RegistryClient, go_proxy_escape,
    parse_url, retract, string,
};

/// The module version's `go.mod` on the module proxy. Best-effort: `None`
/// (unknown) when the proxy has none or cannot be read.
pub(super) async fn go_mod_text(
    artifact: &ArtifactItem,
    client: &RegistryClient<'_>,
) -> Option<String> {
    let (module, version) = (
        artifact.module_path.as_deref()?,
        artifact.version.as_deref()?,
    );
    proxy_go_mod(module, version, client).await.ok().flatten()
}

async fn proxy_go_mod(
    module: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<String>, ArtifactError> {
    let url = parse_url(&format!(
        "https://proxy.golang.org/{}/@v/{}.mod",
        go_proxy_escape(module),
        super::super::util::encode_component(version)
    ))?;
    client.text(ArtifactType::Go, url, true).await
}

/// Whether the module's latest `go.mod` retracts a version.
pub(super) enum Retracted {
    Yes(super::super::types::Retraction),
    No,
    /// The proxy could not say, and why: a version that reads as not
    /// retracted may be.
    Unchecked(String),
}

/// Whether the module's latest `go.mod` retracts the row's version. The go
/// command reads retractions from the latest version's `go.mod`, so that is
/// the one read (`own_mod` when the row is the latest). A proxy that cannot
/// answer leaves the version [`Retracted::Unchecked`], never not retracted;
/// only a cancelled call fails the lookup.
pub(super) async fn go_retraction(
    artifact: &ArtifactItem,
    own_mod: Option<&str>,
    client: &RegistryClient<'_>,
) -> Result<Retracted, ArtifactError> {
    let (Some(module), Some(version)) =
        (artifact.module_path.as_deref(), artifact.version.as_deref())
    else {
        return Ok(Retracted::Unchecked(
            "the row names no module version".into(),
        ));
    };
    let unchecked = |why: String| Ok(Retracted::Unchecked(why));
    let latest = parse_url(&format!(
        "https://proxy.golang.org/{}/@latest",
        go_proxy_escape(module)
    ))?;
    let latest = match client
        .json(ArtifactType::Go, latest, true, None, None, JSON)
        .await
    {
        Ok(Some(latest)) => latest,
        Ok(None) => return unchecked("the module proxy has no @latest for the module".into()),
        Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
        Err(error) => {
            return unchecked(format!(
                "the module proxy's @latest could not be read ({})",
                error.message.trim_end_matches('.')
            ));
        }
    };
    let Some(latest) = string(latest.get("Version")) else {
        return unchecked("the module proxy's @latest names no version".into());
    };
    let go_mod = if latest == version {
        own_mod.map(str::to_owned)
    } else {
        match proxy_go_mod(module, &latest, client).await {
            Ok(go_mod) => go_mod,
            Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
            Err(error) => {
                return unchecked(format!(
                    "the go.mod of the latest version {latest} could not be read ({})",
                    error.message.trim_end_matches('.')
                ));
            }
        }
    };
    let Some(go_mod) = go_mod else {
        return unchecked(format!(
            "the go.mod of the latest version {latest} is unavailable"
        ));
    };
    Ok(retract::retraction(&go_mod, version).map_or(Retracted::No, Retracted::Yes))
}
