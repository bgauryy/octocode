/**
 * Stable error-code taxonomy for LSP-backed tools.
 *
 * Wire format is SCREAMING_SNAKE_CASE so it doubles as a string literal
 * the agent can switch on. Adding a new code is a non-breaking change;
 * renaming one is a breaking change — bump the schema version then.
 *
 * @module lsp/lspErrorCodes
 */
export const LSP_ERROR_CODES = {
  /** No language server binary in PATH for this file's language. */
  LSP_NOT_INSTALLED: 'LSP_NOT_INSTALLED',
  /** The LSP request did not respond inside the request timeout. */
  LSP_TIMEOUT: 'LSP_TIMEOUT',
  /** Server crashed or responded with an error to `initialize`. */
  LSP_INITIALIZE_FAILED: 'LSP_INITIALIZE_FAILED',
  /** Server responded with an LSP error to the request itself. */
  LSP_REQUEST_FAILED: 'LSP_REQUEST_FAILED',
  /** Server responded successfully but the result is empty. */
  LSP_EMPTY: 'LSP_EMPTY',
  /** Active language server does not advertise the requested capability. */
  LSP_CAPABILITY_UNSUPPORTED: 'LSP_CAPABILITY_UNSUPPORTED',
  /** Tool fell back to text-based search (ripgrep) when LSP was unusable. */
  LSP_FALLBACK_TO_TEXT: 'LSP_FALLBACK_TO_TEXT',
  /** The requested symbol could not be resolved near `lineHint`. */
  SYMBOL_NOT_FOUND: 'SYMBOL_NOT_FOUND',
  /** The symbol was found but multiple candidates matched `orderHint`. */
  SYMBOL_AMBIGUOUS: 'SYMBOL_AMBIGUOUS',
  /** A server-provided URI failed validation (T1.5 — fromUri hardening). */
  UNSAFE_URI: 'UNSAFE_URI',
} as const;
