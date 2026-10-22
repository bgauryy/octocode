//! Explicit unread image references. Acquisition stays below text projection:
//! the provider sees original bytes, and the host receives only provenance.
use super::{failure, url};
use crate::execution::ExecutionContext;
use crate::id::clasify_policy as policy;
use crate::tools::judgment::ClassificationError;
use crate::tools::source_capture::SourceReader;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::io::Read;
use std::path::Path;

pub(crate) struct CapturedAssets {
    pub provider: Value,
    pub(crate) provenance: Value,
}

/// Every asset must occur in a value actually sent to JEV. Checking before
/// acquisition avoids reading an unused image (and avoids accidental billing).
pub(crate) fn referenced(value: &Value, key: &str) -> bool {
    match value {
        Value::String(text) => text.contains(key),
        Value::Array(values) => values.iter().any(|value| referenced(value, key)),
        Value::Object(values) => values.values().any(|value| referenced(value, key)),
        _ => false,
    }
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
        total = total.saturating_add(bytes.len());
        if total > policy::MAX_TOTAL_IMAGE_BYTES {
            return Err(too_large(
                "The combined images exceed the per-row byte limit.",
            ));
        }
        let (format, width, height) = dimensions(&bytes)?;
        let saved = store.save_bytes(&bytes, &format!("source.{format}"))?;
        provider.insert(
            key.clone(),
            json!({
                "type":"image", "format":format, "data_base64":STANDARD.encode(&bytes),
            }),
        );
        provenance.insert(
            key.clone(),
            json!({
                "type":"image", "source":origin, "snapshot":saved["snapshot"],
                "format":format, "bytes":bytes.len(), "width":width, "height":height,
                "path":saved["path"], "expiresAt":saved["expiresAt"],
            }),
        );
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
        return url::image(address, dispatcher, context, policy::MAX_IMAGE_BYTES);
    }
    let tool = source["tool"].as_str().unwrap_or_default();
    if !dispatcher.availability().is_available(tool) {
        return Err(failure(
            "classificationContextUnavailable",
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
                    failure(
                        "securityValidationFailed",
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
            if before.len() > policy::MAX_IMAGE_BYTES as u64 {
                return Err(too_large("The image exceeds the per-image byte limit."));
            }
            let mut bytes = Vec::new();
            (&file)
                .take(policy::MAX_IMAGE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| read_failed())?;
            let after = file.metadata().map_err(|_| read_failed())?;
            if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                return Err(failure(
                    "staleSnapshot",
                    "The image changed while being captured; nothing was judged.",
                    "Retry once the image is stable.",
                ));
            }
            if bytes.len() > policy::MAX_IMAGE_BYTES {
                return Err(too_large("The image grew beyond the per-image byte limit."));
            }
            let snapshot = octocode_engine::digest::sha256(&bytes);
            if query["snapshot"]
                .as_str()
                .is_some_and(|expected| expected != snapshot)
            {
                return Err(failure(
                    "staleSnapshot",
                    "The image no longer matches the requested snapshot; nothing was judged.",
                    "Use the saved inspection path from the earlier result, or start a fresh capture.",
                ));
            }
            Ok((
                bytes,
                json!({"tool":"localFetch","query":{"path":path.canonical,"snapshot":snapshot}}),
            ))
        }
        "ghGetFileContent" => {
            let response =
                dispatcher.capture_github_content(query, context, policy::MAX_IMAGE_BYTES)?;
            if response.bytes.len() > policy::MAX_IMAGE_BYTES {
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
    if bytes.len() > policy::MAX_IMAGE_BYTES {
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

fn validate_dimensions(width: usize, height: usize) -> Result<(), ClassificationError> {
    let bounds = || {
        format!(
            "Image width and height must each be {}..={} pixels, and image area {}..={} pixels; received {width}x{height}.",
            policy::MIN_IMAGE_DIMENSION,
            policy::MAX_IMAGE_DIMENSION,
            policy::MIN_IMAGE_PIXELS,
            policy::MAX_IMAGE_PIXELS,
        )
    };
    if width == 0 || height == 0 {
        return Err(invalid(&bounds()));
    }
    let pixels = width.checked_mul(height);
    if width > policy::MAX_IMAGE_DIMENSION
        || height > policy::MAX_IMAGE_DIMENSION
        || pixels.is_none_or(|pixels| pixels > policy::MAX_IMAGE_PIXELS)
    {
        return Err(too_large(&bounds()));
    }
    if width < policy::MIN_IMAGE_DIMENSION
        || height < policy::MIN_IMAGE_DIMENSION
        || pixels.is_some_and(|pixels| pixels < policy::MIN_IMAGE_PIXELS)
    {
        return Err(invalid(&bounds()));
    }
    Ok(())
}

fn check(context: &ExecutionContext) -> Result<(), ClassificationError> {
    context.check().map_err(|stop| {
        let (code, hint) = if matches!(stop, crate::execution::ExecutionError::Cancelled) {
            ("cancelled", crate::response::rows::CANCELLED_HINT)
        } else {
            ("timeout", "Retry with more time.")
        };
        failure(
            code,
            "The call stopped before the images were captured.",
            hint,
        )
    })
}

fn invalid(message: &str) -> ClassificationError {
    failure(
        "invalidClassificationContext",
        message,
        "Reference an explicit PNG or JPEG image by its asset key.",
    )
}
fn too_large(message: &str) -> ClassificationError {
    failure(
        "classificationContextTooLarge",
        message,
        "Use a smaller image or fewer assets; images are never resized or truncated silently.",
    )
}
fn read_failed() -> ClassificationError {
    failure(
        "classificationContextFailed",
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
                "invalidClassificationContext"
            );
        }
        for bytes in [header(8193, 1), header(1, 8193), header(4097, 4097)] {
            assert_eq!(
                dimensions(&bytes).unwrap_err().code,
                "classificationContextTooLarge"
            );
        }
        assert_eq!(
            dimensions(&header(1568, 1568)).unwrap(),
            ("png", 1568, 1568)
        );
    }

    #[test]
    fn dimensions_reject_each_axis_below_the_minimum() {
        for (width, height) in [(256, 400), (400, 256), (279, 400), (400, 279)] {
            let error = dimensions(&header(width, height)).unwrap_err();
            assert_eq!(
                error.code, "invalidClassificationContext",
                "{width}x{height}"
            );
            assert!(error.message.contains("280..=1568"));
            assert!(error.message.contains("78400..=2458624"));
        }
    }

    #[test]
    fn dimensions_reject_area_below_the_minimum() {
        for (width, height) in [(256, 256), (256, 306), (279, 281)] {
            assert_eq!(
                dimensions(&header(width, height)).unwrap_err().code,
                "invalidClassificationContext",
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn dimensions_accept_inclusive_dimension_and_area_edges() {
        for (width, height) in [(280, 280), (280, 1568), (1568, 280), (1568, 1568)] {
            assert_eq!(
                dimensions(&header(width, height)).unwrap(),
                ("png", width as usize, height as usize)
            );
        }
    }

    #[test]
    fn dimensions_reject_maximum_dimension_and_area_overages() {
        for (width, height) in [(1569, 280), (280, 1569), (1569, 1568)] {
            assert_eq!(
                dimensions(&header(width, height)).unwrap_err().code,
                "classificationContextTooLarge",
                "{width}x{height}"
            );
        }
    }

    #[test]
    fn dimensions_reject_checked_pixel_area_overflow() {
        for (width, height) in [(usize::MAX, 2), (2, usize::MAX)] {
            assert_eq!(
                validate_dimensions(width, height).unwrap_err().code,
                "classificationContextTooLarge"
            );
        }
    }

    #[test]
    fn references_are_value_occurrences_including_criteria_not_property_names() {
        assert!(referenced(
            &json!({"criteria":{"yes":"a depicted item in opaque_asset_1"}}),
            "opaque_asset_1"
        ));
        assert!(referenced(
            &json!([{"image":"opaque_asset_1"}]),
            "opaque_asset_1"
        ));
        assert!(!referenced(
            &json!({"opaque_asset_1":"not a value reference"}),
            "opaque_asset_1"
        ));
        assert!(!referenced(&json!(null), "opaque_asset_1"));
    }
}
