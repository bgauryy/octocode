export const OQL_SEARCH_TOOL_NAME = 'oqlSearch';

export const OQL_SEARCH_TOOL_DESCRIPTION =
  'Run an Octocode Query Language (OQL) query or bounded batch through the shared OQL runner. Accepts the same OQL input schema used by the CLI search surface and returns typed result envelopes with diagnostics, provenance, evidence, continuations, and per-row proofGrade. Evidence semantics: answerReady:false means follow next.* continuations for more proof — it is not a failure. proofStatus:"confirmed-by-lsp" means zero references found (safe to inspect); "conflicting-evidence" means LSP found references (symbol IS retained). Run `octocode search --scheme` for the full typed schema before writing queries.';
