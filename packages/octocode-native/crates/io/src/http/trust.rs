//! Explicit PEM roots supplement the operating system trust store.
use super::ClientError;
use reqwest::Certificate;
use std::path::{Path, PathBuf};

pub(super) fn configured_roots() -> Result<Vec<Certificate>, ClientError> {
    let nonempty = |name| std::env::var_os(name).filter(|value| !value.is_empty());
    let file = nonempty("SSL_CERT_FILE").map(PathBuf::from);
    let directories = nonempty("SSL_CERT_DIR")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    roots(file.as_deref(), &directories)
}

fn roots(file: Option<&Path>, directories: &[PathBuf]) -> Result<Vec<Certificate>, ClientError> {
    let mut roots = Vec::new();
    if let Some(file) = file {
        roots.extend(read_bundle(file)?);
    }
    for directory in directories {
        let mut files = std::fs::read_dir(directory)
            .map_err(ClientError::TrustStore)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(ClientError::TrustStore)?;
        files.retain(|path| path.is_file());
        files.sort();
        if files.is_empty() {
            return Err(invalid(
                "Configured TLS certificate directory contains no files",
            ));
        }
        for file in files {
            roots.extend(read_bundle(&file)?);
        }
    }
    Ok(roots)
}

fn read_bundle(path: &Path) -> Result<Vec<Certificate>, ClientError> {
    let pem = std::fs::read(path).map_err(ClientError::TrustStore)?;
    let roots = Certificate::from_pem_bundle(&pem).map_err(ClientError::Tls)?;
    if roots.is_empty() {
        return Err(invalid(
            "Configured TLS certificate file contains no PEM certificates",
        ));
    }
    Ok(roots)
}

fn invalid(message: &'static str) -> ClientError {
    ClientError::TrustStore(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

#[cfg(test)]
mod tests;
