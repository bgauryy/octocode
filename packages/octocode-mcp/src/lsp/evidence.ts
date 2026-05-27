/**
 * Shared LSP evidence attacher.
 *
 * `lsp_call_hierarchy` and `lsp_find_references` both annotate their results
 * with a `kind` / `answerReady` / `complete` / `confidence` block so the bulk
 * runner can lift it to the response envelope. The shapes diverge in two
 * details only:
 *   - the `kind` discriminator (e.g. 'calls' vs 'references')
 *   - the pagination key name ('outputPagination' vs 'pagination')
 * everything else — including the fallback rationale wording — is parameterized.
 */
export function attachLspEvidence<T>(
  result: T,
  opts: {
    kind: 'calls' | 'references';
    paginationKey: 'pagination' | 'outputPagination';
    fallbackReason: string;
  }
): T {
  // Only annotate well-shaped LSP results. Raw error envelopes
  // (`{ isError, message }` or `{ error }`) lack `status` and are returned
  // as-is — tests assert those shapes verbatim.
  const status = (result as { status?: string }).status;
  if (status !== 'hasResults' && status !== 'empty') return result;

  const hasResults = status === 'hasResults';
  const mode = (result as { lspMode?: 'semantic' | 'fallback' }).lspMode;
  const pagination = (
    result as Record<string, { hasMore?: boolean } | undefined>
  )[opts.paginationKey];

  const evidence = {
    kind: opts.kind,
    answerReady: hasResults,
    complete: hasResults && !(pagination?.hasMore ?? false),
    confidence:
      mode === 'semantic'
        ? ('high' as const)
        : mode === 'fallback'
          ? ('low' as const)
          : undefined,
    ...(mode === 'fallback' ? { reason: opts.fallbackReason } : {}),
  };

  // Mutate in place so any non-enumerable raw-chars symbol attached upstream
  // (see attachRawResponseChars) survives.
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}
