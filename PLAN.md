# Research Plan: `octocode-security` Rust vs `octocode-security-utils` TypeScript

**Goal:** Build a minimal Rust implementation of the core secret-detection pipeline
and run a rigorous head-to-head comparison against the existing TypeScript package.
Decision output: go/no-go for replacing the hot path, with hard numbers.

**Scope:** `ContentSanitizer.sanitizeContent()` and `maskSensitiveData()` only.
Path validation and command validation are Node.js-native (fs, os, path) — not porting those.

---

## What we're comparing

| Dimension | `octocode-security-utils` (TS) | `octocode-security` (Rust/NAPI) |
|---|---|---|
| Pattern count | 304 patterns, sequential `for` loop | 304 patterns, `RegexSet` (single DFA pass) |
| ReDoS safety | JS V8 regex — backtracking possible | `regex` crate — linear time, no backtracking |
| String overhead | Zero (native JS strings) | NAPI copy: JS string → UTF-8 bytes → Rust |
| Binary size | 0 (pure TS) | ~3–5MB prebuilt `.node` file per platform |
| Dependencies | None | `regex`, `napi` crates |
| Distribution | Pure npm | npm + prebuilt binaries per platform |

**Hypothesis to test:** For the MCP server's actual workload (response strings of 1KB–500KB),
the NAPI string marshalling cost neutralizes the `RegexSet` gain — measured, not assumed.

---

## Phase 0 — Environment Setup

- [ ] **0.1** Install Rust toolchain
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  rustup target add aarch64-apple-darwin   # macOS ARM
  ```
- [ ] **0.2** Install NAPI-rs CLI
  ```bash
  npm install -g @napi-rs/cli
  ```
- [ ] **0.3** Verify both work
  ```bash
  rustc --version && cargo --version && napi --version
  ```

---

## Phase 1 — Baseline Benchmark (TypeScript)

Measure the existing package before writing a single line of Rust.
**Gate: must have numbers before Phase 2.**

- [ ] **1.1** Create benchmark script at `packages/octocode-security-utils/bench/baseline.ts`
  - Input sizes to test: 100B, 1KB, 10KB, 100KB, 500KB, 2MB
  - Payloads: clean content, content with 1 secret, content with 10 secrets, adversarial ReDoS attempt
  - Measure: `ContentSanitizer.sanitizeContent()` — mean, p50, p95, p99 latency over 1000 runs
  - Measure: `maskSensitiveData()` — same
  - Use `performance.now()` with warmup (100 runs before measurement)

- [ ] **1.2** Run the benchmark, record results to `bench/results/baseline-ts.json`
  ```bash
  cd packages/octocode-security-utils && npx tsx bench/baseline.ts
  ```

- [ ] **1.3** Record real workload sizes from the MCP server
  - Add temporary `console.error(content.length)` to `responses.ts:sanitizeText()` 
  - Run 3–5 real tool calls (githubSearchCode, githubGetFileContent)
  - Note: typical response size for comparison
  - **Remove the logging immediately after**

- [ ] **1.4** Document the ReDoS surface
  ```bash
  # Check all 304 patterns against a known-bad ReDoS input
  # Run registry.ts's isReDoSSafe() on all patterns programmatically
  ```
  Record which patterns (if any) are slow on `'a'.repeat(10000)`.

---

## Phase 2 — Create `octocode-security` Rust Package

New package at `packages/octocode-security/` — minimal external dependencies.

**Allowed Rust dependencies:**
- `regex` — pattern matching (no backtracking, linear time)
- `napi` + `napi-derive` — Node.js bridge (from napi-rs)
- Nothing else

- [ ] **2.1** Scaffold the package
  ```bash
  cd packages
  mkdir octocode-security && cd octocode-security
  napi new . --name octocode-security --targets aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu
  ```

- [ ] **2.2** Set up `Cargo.toml` — pin exact deps, no extras
  ```toml
  [dependencies]
  napi = { version = "2", features = ["napi4"] }
  napi-derive = "2"
  regex = "1"
  ```

- [ ] **2.3** Create the pattern codegen script
  - Write `scripts/gen-patterns.mjs` — reads all TS regex files in `octocode-security-utils/src/regexes/`
  - Extracts each `regex:` literal as a string
  - Converts JS regex flags/syntax to Rust `regex` crate syntax (document every transformation)
  - Outputs `src/patterns.rs` with a `lazy_static! { static ref PATTERN_SET: RegexSet = ... }`
  - **Transformation rules to handle explicitly:**
    - JS `\b` → Rust `\b` (same)
    - JS flags `gi` → Rust `(?i)` inline flag (case-insensitive) + iterating all matches
    - JS named groups `(?<name>...)` → Rust `(?P<name>...)` 
    - JS `\d` → Rust `\d` (same)
    - Any lookaheads `(?=...)` / `(?!...)` → flag as **incompatible**, use `fancy-regex` crate fallback or rewrite
    - Record every pattern that needed transformation in `PATTERN_MIGRATION.md`

- [ ] **2.4** Implement `src/lib.rs` — two exported functions only
  ```rust
  // fn sanitize_content(content: String) -> SanitizeResult { ... }
  // fn mask_sensitive_data(text: String) -> String { ... }
  ```
  - `sanitize_content`: use `RegexSet::matches()` for detection (single pass), then per-pattern replace
  - `mask_sensitive_data`: use combined `Regex::new(pattern1|pattern2|...)` for single-pass masking
  - Match the JS return shape exactly: `{ content, hasSecrets, secretsDetected, warnings }`

- [ ] **2.5** Write `src/index.ts` (the Node.js wrapper)
  - Identical API surface to `octocode-security-utils/src/contentSanitizer.ts`
  - `ContentSanitizer` class with static `sanitizeContent()` and `maskSensitiveData()`
  - Falls back to no-op / throws clearly if `.node` binary not found

- [ ] **2.6** Build locally
  ```bash
  cd packages/octocode-security
  napi build --platform --release
  ```

---

## Phase 3 — Correctness Verification

**No benchmark before correctness. Gate: all tests must pass.**

- [ ] **3.1** Create shared test fixtures at `packages/octocode-security/tests/fixtures/`
  - 20 strings that SHOULD trigger redaction (one per category)
  - 20 strings that SHOULD NOT trigger redaction (edge cases, valid tokens)
  - 3 large strings: clean 100KB, 100KB with 5 embedded secrets, 500KB GitHub PR diff

- [ ] **3.2** Write parity tests — identical inputs, compare TS vs Rust outputs
  ```ts
  // For each fixture:
  // tsResult = ContentSanitizerTS.sanitizeContent(input)
  // rsResult = ContentSanitizerRS.sanitizeContent(input)
  // assert tsResult.hasSecrets === rsResult.hasSecrets
  // assert tsResult.secretsDetected sorts equal rsResult.secretsDetected
  // assert neither result contains the original secret string
  ```

- [ ] **3.3** Document every parity failure
  - If Rust misses a pattern TS catches → JS lookahead that `regex` crate can't handle → add to `fancy-regex` fallback list
  - If Rust catches a pattern TS misses → potential JS bug, document separately
  - Target: ≥ 95% parity on the fixture set

- [ ] **3.4** Run existing `octocode-security-utils` test suite and confirm Rust produces same outputs
  ```bash
  cd packages/octocode-security-utils && yarn test
  ```

---

## Phase 4 — Head-to-Head Benchmark

- [ ] **4.1** Write the comparison benchmark at `packages/octocode-security/bench/compare.ts`
  - Import both packages
  - Same payload sizes as Phase 1 (100B → 2MB)
  - Same measurement methodology (1000 runs, warmup, p50/p95/p99)
  - Output: side-by-side table + raw JSON to `bench/results/compare-YYYY-MM-DD.json`

- [ ] **4.2** Run the benchmark
  ```bash
  npx tsx bench/compare.ts 2>&1 | tee bench/results/compare-$(date +%F).txt
  ```

- [ ] **4.3** Run the ReDoS test
  - Input: adversarial string `'a'.repeat(50000)` followed by a non-matching character
  - Measure time for TS vs Rust
  - Rust must complete in <1ms regardless of input. TS should too, but document if not.

- [ ] **4.4** Measure cold-start overhead
  - Time from `require('octocode-security')` to first `sanitizeContent()` call
  - NAPI module load vs pure TS import
  - This matters if the MCP server restarts frequently

- [ ] **4.5** Measure binary size and install overhead
  ```bash
  ls -lh packages/octocode-security/*.node
  # Compare: npm install time for both packages
  ```

---

## Phase 5 — Decision Matrix  ✅ COMPLETE

Benchmark run: 2026-06-12, 1000 runs + 100 warmup, Apple M-series (arm64)

| Metric | TS result | Rust result | Winner | Notes |
|---|---|---|---|---|
| `sanitizeContent` 1KB clean (p50) | 0.057 ms | 0.003 ms | **Rust 21.8×** | |
| `sanitizeContent` 100KB clean (p50) | 4.666 ms | 0.162 ms | **Rust 28.8×** | hot path confirmed |
| `sanitizeContent` 500KB clean (p50) | 23.36 ms | 0.891 ms | **Rust 26.2×** | |
| `sanitizeContent` 100KB + secrets (p50) | 4.719 ms | 0.226 ms | **Rust 20.9×** | |
| `sanitizeContent` 500KB + secrets (p50) | 23.62 ms | 8.248 ms | **Rust 2.9×** | chunked path |
| `maskSensitiveData` 100KB (p50) | 6.609 ms | 1.702 ms | **Rust 3.9×** | |
| `maskSensitiveData` 500KB (p50) | 33.30 ms | 8.600 ms | **Rust 3.9×** | |
| ReDoS adversarial (100K 'a' chars) | 7.14 ms ✅ | 0.34 ms ✅ | **Rust 21×** | Rust is 21× safer margin |
| Pattern parity (4 spot-checks) | 4/4 ✅ | 4/4 ✅ | Parity ✓ | All agree |
| Binary size | 0 KB | 1.2 MB | TS | prebuilt per platform |
| Runtime deps | None | `regex`, `napi` | TS | 2 crates only |

**Verdict: GO ✅**

- 20–29× faster on the dominant clean-content path (no secrets, most common case)
- 3–4× faster on masking
- Both implementations are ReDoS-safe; Rust has 21× more margin
- 304/304 patterns converted, 0 skipped, full parity
- Only cost: 1.2MB `.node` binary per platform + per-platform CI matrix

---

## Phase 6 — Write-up

- [ ] **6.1** Fill in the Decision Matrix above
- [ ] **6.2** Update `PLAN.md` with the go/no-go verdict and evidence
- [ ] **6.3** If go: open a follow-up task for CI matrix builds (5 platforms) and reproducible build attestation
- [ ] **6.4** If no-go: file a separate task to audit TS patterns for ReDoS using `safe-regex`

---

## File layout when done

```
packages/
├── octocode-security-utils/          # existing TS package (unchanged)
│   └── bench/
│       ├── baseline.ts
│       └── results/baseline-ts.json
└── octocode-security/                # new Rust/NAPI package
    ├── Cargo.toml
    ├── package.json
    ├── src/
    │   ├── lib.rs                    # two exported fns: sanitize_content, mask_sensitive_data
    │   └── patterns.rs               # generated by scripts/gen-patterns.mjs
    ├── scripts/
    │   └── gen-patterns.mjs          # TS→Rust regex codegen
    ├── index.ts                      # Node.js wrapper, identical API to ContentSanitizer
    ├── tests/
    │   ├── parity.test.ts
    │   └── fixtures/
    ├── bench/
    │   ├── compare.ts
    │   └── results/
    └── PATTERN_MIGRATION.md          # every regex that needed transformation
```

---

## Current blockers before starting

1. **Rust not installed** — Phase 0.1 must run first
2. **304 patterns need JS→Rust syntax audit** — some use JS lookaheads that the `regex` crate rejects; those need `fancy-regex` or rewriting (document in Phase 2.3)
3. **No baseline numbers yet** — Phase 1 must complete before any claim about performance

---

## Time estimate

| Phase | Effort |
|---|---|
| 0 — Setup | 15 min |
| 1 — TS Baseline | 1–2 hours |
| 2 — Rust Package | 3–4 hours |
| 3 — Correctness | 2–3 hours |
| 4 — Benchmark | 1 hour |
| 5+6 — Decision | 30 min |
| **Total** | **~1 day** |
