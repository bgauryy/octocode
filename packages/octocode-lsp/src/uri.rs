use napi::{Error, Result, Status};
use std::path::{Path, PathBuf};
use url::Url;

pub fn path_to_uri(path: &str) -> Result<String> {
    Url::from_file_path(Path::new(path))
        .map(|uri| uri.to_string())
        .map_err(|()| Error::new(Status::InvalidArg, format!("Invalid file path: {path}")))
}

pub fn uri_to_path(uri: &str) -> Result<String> {
    let parsed = Url::parse(uri)
        .map_err(|err| Error::new(Status::InvalidArg, format!("Invalid URI: {err}")))?;
    parsed
        .to_file_path()
        .map(path_to_string)
        .map_err(|()| Error::new(Status::InvalidArg, format!("URI is not a file URI: {uri}")))
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}
