//! Explicit unread image references. Acquisition stays below text projection:
//! the provider sees original bytes, and the host receives only provenance.
use super::url;
use crate::execution::ExecutionContext;
use crate::id::clasify_policy;
use crate::tools::judgment::{ClassificationError, referenced};
use crate::tools::source_capture::SourceReader;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value, json};
use std::path::Path;

mod fit;

pub(crate) struct CapturedAssets {
    pub provider: Value,
    pub(crate) provenance: Value,
}

pub(crate) fn capture(
    assets: &Value,
    state: &Value,
    questions: &[Value],
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<CapturedAssets, ClassificationError> {
    let Some(assets) = assets.as_object() else {
        return Err(invalid("Image assets must be a keyed map."));
    };
    for key in assets.keys() {
        if !referenced(state, key) && !questions.iter().any(|question| referenced(question, key)) {
            return Err(invalid(&format!(
                "Image asset {key} is not referenced in state or questions; nothing was judged."
            )));
        }
    }
    let store = url::Store::open(dispatcher)?;
    let mut provider = Map::new();
    let mut provenance = Map::new();
    let mut total = 0usize;
    for (key, asset) in assets {
        check(context)?;
        let (bytes, origin) = acquire(&asset["source"], dispatcher, context)?;
        let Fitted {
            format,
            width,
            height,
            bytes,
            source,
        } = fit(bytes)?;
        total = total.saturating_add(bytes.len());
        if total > clasify_policy::MAX_TOTAL_IMAGE_BYTES {
            return Err(too_large(
                "The combined images exceed the per-row byte limit.",
            ));
        }
        // The saved snapshot is the judged image, so box fractions map onto it.
        let saved = store.save_bytes(&bytes, &format!("source.{format}"))?;
        provider.insert(
            key.clone(),
            json!({
                "type":"image", "format":format, "data_base64":STANDARD.encode(&bytes),
            }),
        );
        let mut provenance_row = json!({
            "source":origin, "snapshot":saved["snapshot"],
            "format":format, "bytes":bytes.len(), "width":width, "height":height,
            "path":saved["path"], "expiresAt":saved["expiresAt"],
        });
        if let Some((source_width, source_height)) = source {
            provenance_row["sourceWidth"] = json!(source_width);
            provenance_row["sourceHeight"] = json!(source_height);
        }
        provenance.insert(key.clone(), provenance_row);
    }
    check(context)?;
    Ok(CapturedAssets {
        provider: Value::Object(provider),
        provenance: Value::Object(provenance),
    })
}

fn acquire(
    source: &Value,
    dispatcher: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<(Vec<u8>, Value), ClassificationError> {
    if let Some(address) = source["url"].as_str() {
        return url::image(
            address,
            dispatcher,
            context,
            clasify_policy::MAX_IMAGE_BYTES,
        );
    }
    let tool = source["tool"].as_str().unwrap_or_default();
    if !dispatcher.availability().is_available(tool) {
        return Err(ClassificationError::new(
            ErrorCode::ClassificationContextUnavailable,
            format!("The image reader {tool} is unavailable."),
            "Enable the referenced reader before classifying its images.",
        ));
    }
    let query = &source["query"];
    match tool {
        "localFetch" => {
            let path = dispatcher
                .paths()
                .validate_read(Path::new(query["path"].as_str().unwrap_or_default()))
                .map_err(|error| {
                    ClassificationError::new(
                        ErrorCode::SecurityValidationFailed,
                        error.to_string(),
                        "Read an allowed regular image file.",
                    )
                })?;
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let file = options.open(&path.canonical).map_err(|_| read_failed())?;
            let before = file.metadata().map_err(|_| read_failed())?;
            if !before.is_file() {
                return Err(invalid("The image path is not a regular file."));
            }
            if before.len() > clasify_policy::MAX_IMAGE_BYTES as u64 {
                return Err(too_large("The image exceeds the per-image byte limit."));
            }
            let read =
                octocode_io::read::read_limited(&file, clasify_policy::MAX_IMAGE_BYTES as u64)
                    .map_err(|_| read_failed())?;
            let after = file.metadata().map_err(|_| read_failed())?;
            if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                return Err(ClassificationError::new(
                    ErrorCode::StaleSnapshot,
                    "The image changed while being captured; nothing was judged.",
                    "Retry once the image is stable.",
                ));
            }
            let Some(bytes) = read else {
                return Err(too_large("The image grew beyond the per-image byte limit."));
            };
            Ok((
                bytes,
                json!({"tool":"localFetch","query":{"path":path.canonical}}),
            ))
        }
        "ghGetFileContent" => {
            let response = dispatcher.capture_github_content(
                query,
                context,
                clasify_policy::MAX_IMAGE_BYTES,
            )?;
            if response.bytes.len() > clasify_policy::MAX_IMAGE_BYTES {
                return Err(too_large(
                    "The GitHub image exceeds the per-image byte limit.",
                ));
            }
            let mut query = query.clone();
            query["ref"] = json!(response.resolved_ref);
            Ok((
                response.bytes.to_vec(),
                json!({"tool":"ghGetFileContent","query":query}),
            ))
        }
        _ => Err(invalid(
            "The image source must name a local, GitHub or HTTPS image.",
        )),
    }
}

fn dimensions(bytes: &[u8]) -> Result<(&'static str, usize, usize), ClassificationError> {
    if bytes.len() > clasify_policy::MAX_IMAGE_BYTES {
        return Err(too_large("The image exceeds the per-image byte limit."));
    }
    let format = match imagesize::image_type(bytes) {
        Ok(imagesize::ImageType::Png)
            if bytes.starts_with(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR") && bytes.len() >= 33 =>
        {
            "png"
        }
        Ok(imagesize::ImageType::Jpeg) => "jpeg",
        _ => return Err(invalid("The asset is not a PNG or JPEG image.")),
    };
    let size = imagesize::blob_size(bytes)
        .map_err(|_| invalid("The image header is incomplete or invalid."))?;
    validate_dimensions(size.width, size.height)?;
    Ok((format, size.width, size.height))
}

/// Source images this large are refused before decoding; anything smaller
/// is fitted into the provider window.
fn validate_dimensions(width: usize, height: usize) -> Result<(), ClassificationError> {
    let max = clasify_policy::MAX_SOURCE_IMAGE_DIMENSION;
    let bounds = || {
        format!("Image width and height must each be 1..={max} pixels; received {width}x{height}.")
    };
    if width == 0 || height == 0 {
        return Err(invalid(&bounds()));
    }
    if width > max || height > max {
        return Err(too_large(&bounds()));
    }
    Ok(())
}

/// The image the provider judges, and the source size when it differs.
#[derive(Debug, PartialEq)]
struct Fitted {
    format: &'static str,
    width: usize,
    height: usize,
    bytes: Vec<u8>,
    source: Option<(usize, usize)>,
}

/// Admits an image already inside the provider window as-is; otherwise
/// scales it down (aspect kept) and pads a side below the minimum. A square
/// window bounds the area as well as each side.
fn fit(bytes: Vec<u8>) -> Result<Fitted, ClassificationError> {
    let (format, width, height) = dimensions(&bytes)?;
    let (min, max) = (
        clasify_policy::MIN_IMAGE_DIMENSION,
        clasify_policy::MAX_IMAGE_DIMENSION,
    );
    debug_assert!(min * min >= clasify_policy::MIN_IMAGE_PIXELS);
    debug_assert!(max * max <= clasify_policy::MAX_IMAGE_PIXELS);
    if (min..=max).contains(&width) && (min..=max).contains(&height) {
        return Ok(Fitted {
            format,
            width,
            height,
            bytes,
            source: None,
        });
    }
    let (fitted_width, fitted_height, fitted) = fit::fit(&bytes, format, width, height, min, max)?;
    if fitted.len() > clasify_policy::MAX_IMAGE_BYTES {
        return Err(too_large(
            "The fitted image exceeds the per-image byte limit.",
        ));
    }
    Ok(Fitted {
        format: "png",
        width: fitted_width,
        height: fitted_height,
        bytes: fitted,
        source: Some((width, height)),
    })
}

fn check(context: &ExecutionContext) -> Result<(), ClassificationError> {
    context.check().map_err(|stop| {
        let (code, hint) = if matches!(stop, crate::execution::ExecutionError::Cancelled) {
            (ErrorCode::Cancelled, crate::response::rows::CANCELLED_HINT)
        } else {
            (ErrorCode::Timeout, "Retry with more time.")
        };
        ClassificationError::new(
            code,
            "The call stopped before the images were captured.",
            hint,
        )
    })
}

fn invalid(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::InvalidClassificationContext,
        message,
        "Reference an explicit PNG or JPEG image by its asset key.",
    )
}
fn too_large(message: &str) -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextTooLarge,
        message,
        "Use a smaller image or fewer assets; images are never resized or truncated silently.",
    )
}
fn read_failed() -> ClassificationError {
    ClassificationError::new(
        ErrorCode::ClassificationContextFailed,
        "The image could not be read completely.",
        "Check the file path and permissions.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.resize(33, 0);
        bytes
    }

    #[test]
    fn dimensions_reject_unknown_truncated_zero_and_excessive_headers() {
        for bytes in [
            vec![],
            vec![0xff, 0xd8, 0xff],
            b"GIF89a".to_vec(),
            header(0, 1),
            header(1, 0),
        ] {
            assert_eq!(
                dimensions(&bytes).unwrap_err().code,
                ErrorCode::InvalidClassificationContext
            );
        }
        for bytes in [header(8193, 1), header(1, 8193), header(8193, 8193)] {
            assert_eq!(
                dimensions(&bytes).unwrap_err().code,
                ErrorCode::ClassificationContextTooLarge
            );
        }
        assert_eq!(
            dimensions(&header(1568, 1568)).unwrap(),
            ("png", 1568, 1568)
        );
    }

    #[test]
    fn fit_rejects_short_images_whose_pixels_do_not_decode() {
        assert_eq!(
            fit(header(500, 104)).unwrap_err().code,
            ErrorCode::InvalidClassificationContext
        );
    }

    #[test]
    fn fit_rejects_large_images_whose_pixels_do_not_decode() {
        assert_eq!(
            fit(header(1600, 900)).unwrap_err().code,
            ErrorCode::InvalidClassificationContext
        );
    }

    #[test]
    fn dimensions_accept_every_side_up_to_the_source_maximum() {
        for (width, height) in [(1, 1), (280, 1568), (1569, 280), (8192, 8192)] {
            assert_eq!(
                dimensions(&header(width, height)).unwrap(),
                ("png", width as usize, height as usize)
            );
        }
    }

    #[test]
    fn dimensions_reject_sides_beyond_the_source_maximum() {
        for (width, height) in [(usize::MAX, 2), (2, usize::MAX), (8193, 8192)] {
            assert_eq!(
                validate_dimensions(width, height).unwrap_err().code,
                ErrorCode::ClassificationContextTooLarge
            );
        }
    }
}
