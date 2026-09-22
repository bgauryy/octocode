# Adding a clasify vendor

The `clasify` tool's transport, batch scheduler, context capture, and paging
engine are all vendor-agnostic. Adding a new classification API is exactly
**four steps** — no changes to those layers.

Reference implementation: [`src/providers/classification/jev.rs`](../../crates/runtime/src/providers/classification/jev.rs)

---

## 1 — Create the vendor file

```
src/providers/classification/<vendor>.rs
```

Implement `ClassificationProvider` for your vendor.
Every method is documented in [`mod.rs`](../../crates/runtime/src/providers/classification/mod.rs).

```rust
// src/providers/classification/myvendor.rs
use super::ClassificationProvider;
use octocode_engine::jev::JevError;
use serde_json::{Value, json};

pub struct MyVendor;
pub static MY_VENDOR: MyVendor = MyVendor;

impl ClassificationProvider for MyVendor {
    fn id(&self) -> &'static str { "myvendor" }

    /// Vendor-specific key var accepted alongside OCTOCODE_CLASSIFICATION_API.
    fn key_env(&self) -> &'static str { "OCTOCODE_MYVENDOR_KEY" }

    fn default_host(&self) -> &'static str { "https://api.myvendor.ai" }
    fn default_model(&self) -> &'static str { "myvendor-v1" }
    fn endpoint_path(&self) -> &'static str { "v1/classify" }
    fn docs_url(&self) -> &'static str { "https://docs.myvendor.ai/getting-started" }

    // ── Wire format ──────────────────────────────────────────────────────────
    // Shape the JSON body however your API expects.

    fn build_request(&self, state: &Value, question: &Value, model: &str) -> Value {
        json!({ "model": model, "context": state, "query": question })
    }

    fn build_batch_request(
        &self,
        state: &Value,
        questions: &[(usize, &Value)],
        model: &str,
    ) -> Value {
        let qs: serde_json::Map<String, Value> = questions
            .iter()
            .map(|(i, q)| (format!("q{i}"), (*q).clone()))
            .collect();
        json!({ "model": model, "context": state, "queries": qs })
    }

    // ── Response extraction ──────────────────────────────────────────────────
    // Tell the engine where to find the answer in the vendor's response body.

    fn extract_answer<'a>(&self, response: &'a Value) -> Option<&'a Value> {
        response.get("result")
    }

    fn batch_answer_key(&self, index: usize) -> String {
        format!("q{index}")
    }

    // ── Validation ───────────────────────────────────────────────────────────
    // If your vendor uses the Jev schema, delegate; otherwise write your own.

    fn validate_response(&self, request: &Value, response: &Value) -> Result<(), JevError> {
        octocode_engine::jev::validate_response(request, response)
    }
}
```

---

## 2 — Register the module

In `src/providers/classification/mod.rs`, add:

```rust
pub mod myvendor;
```

---

## 3 — Add an arm to `provider_for`

In the same file:

```rust
pub fn provider_for(vendor: &str) -> &'static dyn ClassificationProvider {
    match vendor {
        "jev"      => &jev::JEV,
        "myvendor" => &myvendor::MY_VENDOR,   // ← add this
        _          => &jev::JEV,              // fallback guard, keep last
    }
}
```

---

## 4 — Extend the config contract

The `classification.type` enum lives in the shared `octocode-core` contracts repo
(`../octocode-mcp-host/packages/octocode-core`).  Add `"myvendor"` to the enum,
then regenerate:

```bash
yarn workspace @octocodeai/octocode-native contracts:regen
```

---

## Done — what you get for free

| Layer | File | What it does |
|---|---|---|
| Transport | `tools/clasify/transport.rs` | HTTP POST, retries, budget, error codes |
| Single call | `tools/clasify/mod.rs` | Preflight → `build_request` → `extract_answer` → project |
| Batching | `tools/clasify/batch.rs` | Groups questions → `build_batch_request` → `batch_answer_key` |
| Paging | `runtime/clasify_batch.rs` | Resource-major paging, concurrency semaphore |
| Context | `runtime/clasify_context.rs` | Context capture, receipt, continuation logic |

None of those files need touching.  The only vendor-specific code lives in your
new `providers/classification/myvendor.rs`.

---

## Checklist

- [ ] `src/providers/classification/myvendor.rs` created
- [ ] `pub mod myvendor;` added to `mod.rs`
- [ ] `"myvendor"` arm added to `provider_for()`
- [ ] `"myvendor"` added to the `classification.type` enum in `octocode-core`
- [ ] `contracts:regen` run and output committed
- [ ] At least one integration test exercises `clasify` with a mock of your API
      (see `tests/runtime_clasify.rs` for the existing Jev fixture pattern)
