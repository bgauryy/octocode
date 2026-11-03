// A live semantic assertion may accept the server's explicit indexing reattempt.
// Each attempt drains required pagination first; partial evidence remains in receipts.
const rowsOf = pages => pages.flatMap(page => page.json?.results ?? []);
const errorPage = page => page.isError || page.json?.errorCode || page.json?.error
  || page.json?.results?.some(row => row.status === 'error' || row.data?.errorCode);

export function semanticPagesComplete(pages) {
  return pages.length > 0 && pages.every(page => !errorPage(page)
    && page.json?.results?.length > 0 && page.json.results.every(row => !row.data?.isPartial));
}

export function indexingRetry(pages) {
  if (pages.some(errorPage)) return null;
  const partial = rowsOf(pages).filter(row => row.data?.isPartial);
  if (!partial.length || partial.some(row => !row.data.partialReasons?.length
    || row.data.partialReasons.some(reason => reason !== 'languageServerIndexing'))) return null;
  const calls = new Map();
  for (const row of partial) {
    const retries = (row.data.related ?? []).filter(call => call.kind === 'related' && call.tool === 'lspSearch' && call.input);
    if (!retries.length) return null;
    for (const call of retries) calls.set(JSON.stringify(call), call);
  }
  return calls.size === 1 ? calls.values().next().value : null;
}

/** `explore` finishes response/tool pages before returning, including any failures. */
export async function settleSemanticQuery(tool, input, explore) {
  const attempts = [];
  let next = { tool, input };
  while (next) {
    const attempt = await explore(next.tool, next.input);
    attempts.push(attempt);
    next = attempt.failures.length ? null : indexingRetry(attempt.pages);
  }
  return { attempts, pages: attempts.at(-1).pages, failures: attempts.flatMap(attempt => attempt.failures) };
}
