//! Where the original image sits on the canvas the provider judged, and the
//! mapping of a provider box back onto the original.
use super::CapturedAssets;
use crate::tools::judgment::referenced;
use serde_json::{Value, json};

/// The source's rectangle on the fitted canvas, in canvas pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    pub canvas_width: usize,
    pub canvas_height: usize,
    pub left: usize,
    pub top: usize,
    pub width: usize,
    pub height: usize,
}

impl Frame {
    /// One axis: a canvas fraction onto the source, clamped into the image.
    fn source(fraction: f64, canvas: usize, offset: usize, content: usize) -> f64 {
        ((fraction * canvas as f64 - offset as f64) / content as f64).clamp(0.0, 1.0)
    }

    /// A provider `box` (fractions of the canvas) as fractions of the
    /// original image. `None` when the box lacks an edge.
    pub(crate) fn source_box(&self, canvas_box: &Value) -> Option<Value> {
        let edge = |name: &str| canvas_box[name].as_f64();
        let vertical =
            |name| edge(name).map(|f| Self::source(f, self.canvas_height, self.top, self.height));
        let horizontal =
            |name| edge(name).map(|f| Self::source(f, self.canvas_width, self.left, self.width));
        Some(json!({
            "top": vertical("top")?,
            "bottom": vertical("bottom")?,
            "left": horizontal("left")?,
            "right": horizontal("right")?,
        }))
    }
}

impl CapturedAssets {
    /// The asset a question is about: the row's only asset, else the one
    /// asset key it cites. `None` when it cites none or several.
    fn image_of(&self, question: &Value) -> Option<&str> {
        let mut keys = self.provenance.as_object()?.keys();
        let only = keys.next().filter(|_| keys.next().is_none());
        if only.is_some() {
            return only.map(String::as_str);
        }
        let mut cited = self
            .provenance
            .as_object()?
            .keys()
            .filter(|key| referenced(question, key));
        cited
            .next()
            .filter(|_| cited.next().is_none())
            .map(String::as_str)
    }

    /// Adds `sourceBox` to each box answer whose image was fitted: the
    /// provider's box mapped onto the original. Other answers, and boxes
    /// whose image is unfitted or ambiguous, stay as the provider returned.
    pub(crate) fn attach_source_boxes(&self, questions: &[Value], answers: &mut [Value]) {
        for (question, answer) in questions.iter().zip(answers) {
            if answer["type"] != "box" {
                continue;
            }
            let mapped = self
                .image_of(question)
                .and_then(|key| self.frames.get(key))
                .and_then(|frame| frame.source_box(&answer["box"]));
            if let Some(mapped) = mapped {
                answer["sourceBox"] = mapped;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const PADDED: Frame = Frame {
        canvas_width: 280,
        canvas_height: 280,
        left: 80,
        top: 100,
        width: 120,
        height: 80,
    };

    fn assets(keys: &[&str], fitted: &[&str]) -> CapturedAssets {
        CapturedAssets {
            provider: json!({}),
            provenance: keys
                .iter()
                .map(|key| (key.to_string(), json!({})))
                .collect(),
            frames: fitted
                .iter()
                .map(|key| (key.to_string(), PADDED))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn answer() -> Value {
        json!({"type":"box","box":{"top":0.36,"bottom":0.46,"left":0.6,"right":0.65},"confidence":0.9})
    }

    fn mapped(assets: &CapturedAssets, question: &str) -> Vec<Value> {
        let mut answers = vec![json!({"type":"noul","noul":0.5}), answer()];
        assets.attach_source_boxes(&[json!({"type":"noul"}), json!(question)], &mut answers);
        answers
    }

    fn close(actual: &Value, expected: [f64; 4]) {
        for (name, want) in ["top", "bottom", "left", "right"].into_iter().zip(expected) {
            let got = actual[name].as_f64().unwrap();
            assert!((got - want).abs() < 0.005, "{name}: {got} vs {want}");
        }
    }

    #[test]
    fn a_padded_small_image_maps_the_canvas_box_onto_the_original() {
        let mapped = PADDED
            .source_box(&json!({"top":0.36,"bottom":0.46,"left":0.60,"right":0.65}))
            .unwrap();
        close(&mapped, [0.01, 0.36, 0.733, 0.85]);
    }

    #[test]
    fn a_scaled_then_padded_banner_maps_only_the_padded_axis() {
        let frame = Frame {
            canvas_width: 1568,
            canvas_height: 280,
            left: 0,
            top: 101,
            width: 1568,
            height: 78,
        };
        let mapped = frame
            .source_box(&json!({"top":0.41,"bottom":0.58,"left":0.2,"right":0.9}))
            .unwrap();
        close(&mapped, [0.177, 0.787, 0.2, 0.9]);
    }

    #[test]
    fn a_box_inside_the_padding_clamps_and_an_incomplete_box_maps_to_nothing() {
        let mapped = PADDED
            .source_box(&json!({"top":0.0,"bottom":0.2,"left":0.9,"right":1.0}))
            .unwrap();
        close(&mapped, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(PADDED.source_box(&json!({"top":0.1})), None);
    }

    #[test]
    fn a_boxs_image_is_the_only_asset_or_the_single_one_it_cites() {
        let one = mapped(&assets(&["img"], &["img"]), "the square");
        assert_eq!(one[0].get("sourceBox"), None);
        close(&one[1]["sourceBox"], [0.01, 0.36, 0.733, 0.85]);
        let two = assets(&["a", "b"], &["a", "b"]);
        close(
            &mapped(&two, "the square in b")[1]["sourceBox"],
            [0.01, 0.36, 0.733, 0.85],
        );
        for question in ["the square", "the square in a or b"] {
            assert_eq!(
                mapped(&two, question)[1].get("sourceBox"),
                None,
                "{question}"
            );
        }
    }

    #[test]
    fn an_unfitted_image_gets_no_source_box() {
        assert_eq!(mapped(&assets(&["img"], &[]), "x")[1], answer());
        let mixed = assets(&["a", "b"], &["a"]);
        assert_eq!(mapped(&mixed, "in b")[1], answer());
        assert!(mapped(&mixed, "in a")[1].get("sourceBox").is_some());
    }
}
