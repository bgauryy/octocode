/**
 * ids — the one definition of the short display form of agent/session ids.
 *
 * Every surface that shows an id (ledger, inbox, worktree branches, rewind,
 * autocomplete) uses the same 8-char prefix; keep the constant here so the
 * display form can never drift between surfaces.
 */

/** First 8 chars of an agent/session id — the canonical short display form. */
export function shortId(id: string): string {
  return id.slice(0, 8);
}
