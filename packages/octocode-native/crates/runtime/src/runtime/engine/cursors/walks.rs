//! Several rows' walks on one handle.
use super::{Pending, SavedWork};
use crate::id::ToolId;

impl Pending {
    /// The one handle that continues every walk in `handles` (each a tool
    /// continuation this output issued): the walk's own for one, else a new
    /// one whose rows run together and keep their queries' indices.
    pub fn walk_handle(&mut self, tool: ToolId, handles: &[String]) -> Option<String> {
        if let [handle] = handles {
            return Some(handle.clone());
        }
        let (mut queries, mut indices, mut chars) = (Vec::new(), Vec::new(), None);
        for handle in handles {
            let member = self.issued.get(handle)?;
            let SavedWork::ToolResume { index: Some(index) } = member.work else {
                return None;
            };
            let rows = member.input.get("queries")?.as_array()?;
            indices.extend(std::iter::repeat_n(index, rows.len()));
            queries.extend(rows.iter().cloned());
            chars = chars.or_else(|| member.input.get("responseChars").cloned());
        }
        let mut input = serde_json::json!({"queries": queries});
        if let Some(chars) = chars {
            input["responseChars"] = chars;
        }
        Some(self.insert(tool, input, SavedWork::Rows { indices }))
    }
}
