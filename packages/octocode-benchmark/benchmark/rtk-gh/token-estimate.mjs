// Token-usage measurement for rtk-gh benchmark runs.
//
// Prefers a real BPE tokenizer (e.g. `gpt-tokenizer`) if it is installed in
// node_modules; otherwise falls back to a documented heuristic so token
// numbers are still available with no network access. The active method is
// exposed via `tokenizerMethod()` and MUST be recorded once per run (in
// manifest.json) — never mix methods within one run's comparison.
//
// Heuristic (used when no real tokenizer is installed):
// OpenAI's published rule of thumb is ~4 chars/token for English text. A
// flat `text.length / 4` is a poor fit for code/JSON/paths because it
// over-counts long runs of punctuation and under-counts long identifiers.
// Instead we split into word-ish units (`[A-Za-z0-9_]+` or a single other
// character) and charge each unit `ceil(len / 4)` tokens, minimum 1. This
// keeps short common tokens (words, punctuation, single symbols) at 1 token
// each — matching real BPE vocabularies — while still charging long
// identifiers/hashes/base64 blobs proportionally to their length.
let realEncode = null;
let realTokenizerName = null;
try {
  // Optional dependency — not installed by default in this sandbox (no
  // network access to add it). If a future environment `yarn add`s it, this
  // silently upgrades every run to exact token counts with no code change.
  const mod = await import('gpt-tokenizer');
  const candidate = mod.encode ?? mod.default?.encode;
  if (typeof candidate === 'function') {
    realEncode = candidate;
    realTokenizerName = 'gpt-tokenizer/cl100k_base';
  }
} catch {
  // Not installed — heuristic fallback below.
}

const HEURISTIC_NAME = 'heuristic/word-punct-div4-v1';
const UNIT_RE = /[A-Za-z0-9_]+|[^\sA-Za-z0-9_]/g;

function heuristicTokens(text) {
  const units = text.match(UNIT_RE);
  if (!units) return 0;
  let tokens = 0;
  for (const u of units) tokens += Math.max(1, Math.ceil(u.length / 4));
  return tokens;
}

export function tokenizerMethod() {
  return realTokenizerName ?? HEURISTIC_NAME;
}

export function isRealTokenizer() {
  return realEncode !== null;
}

export function estimateTokens(text) {
  if (!text) return 0;
  if (realEncode) {
    try {
      return realEncode(text).length;
    } catch {
      // Rare encode error on pathological input — fall back rather than crash a run.
    }
  }
  return heuristicTokens(text);
}
