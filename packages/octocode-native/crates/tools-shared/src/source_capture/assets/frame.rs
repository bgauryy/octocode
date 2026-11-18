//! Where the original image sits on the canvas the provider judged, and the
//! mapping of a provider box back onto the original.
use super::CapturedAssets;
use crate::judgment::referenced;
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

    /// Whether the source fills the whole canvas (only scaled, never padded):
    /// a canvas fraction is then already a fraction of the original.
    fn fills_canvas(&self) -> bool {
        (self.left, self.top, self.width, self.height)
            == (0, 0, self.canvas_width, self.canvas_height)
    }

    /// A provider `box` (fractions of the canvas) as fractions of the
    /// original image. `None` when the box lacks an edge or, once clamped
    /// into the image, has no area (it lay in the padding).
    pub(crate) fn source_box(&self, canvas_box: &Value) -> Option<Value> {
        let edge = |name: &str| canvas_box[name].as_f64();
        let vertical =
            |name| edge(name).map(|f| Self::source(f, self.canvas_height, self.top, self.height));
        let horizontal =
            |name| edge(name).map(|f| Self::source(f, self.canvas_width, self.left, self.width));
        let (top, bottom) = (vertical("top")?, vertical("bottom")?);
        let (left, right) = (horizontal("left")?, horizontal("right")?);
        (bottom > top && right > left)
            .then(|| json!({"top": top, "bottom": bottom, "left": left, "right": right}))
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

    /// Adds `sourceBox` to each box answer whose image was padded: the
    /// provider's box mapped onto the original. Other answers, and boxes
    /// whose image is unfitted, only scaled (its `box` is already on the
    /// original) or ambiguous, stay as the provider returned.
    pub fn attach_source_boxes(&self, questions: &[Value], answers: &mut [Value]) {
        for (question, answer) in questions.iter().zip(answers) {
            if answer["type"] != "box" {
                continue;
            }
            let mapped = self
                .image_of(question)
                .and_then(|key| self.frames.get(key))
                .filter(|frame| !frame.fills_canvas())
                .and_then(|frame| frame.source_box(&answer["box"]));
            if let Some(mapped) = mapped {
                answer["sourceBox"] = mapped;
            }
        }
    }
}

#[cfg(test)]
mod tests;
