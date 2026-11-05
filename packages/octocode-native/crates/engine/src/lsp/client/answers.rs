//! Provider evidence owned by one operation or an issued continuation.
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct ProviderAnswers(pub(super) BTreeMap<String, Arc<Value>>);

impl Serialize for ProviderAnswers {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value.as_ref())?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for ProviderAnswers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::<String, Value>::deserialize(deserializer).map(|entries| {
            Self(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, Arc::new(value)))
                    .collect(),
            )
        })
    }
}

impl super::NativeLspClient {
    /// Drop the answers this response recorded: documents opened since changed
    /// what an index-backed server answers, so a recorded answer no longer
    /// stands for asking. Outside a response scope there is nothing to drop.
    pub fn forget_recorded_answers(&self) {
        let _ = super::RESPONSE_SCOPE.try_with(|scope| {
            if let Some(answers) = scope.borrow().answers.as_ref() {
                answers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
                    .clear();
            }
        });
    }
}
