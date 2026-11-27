// What a navigation to one URL came to: the main document's HTTP outcome and the page left
// behind, reported plainly even when Chrome shows its own error page.
import type { NetworkRecord } from './live-har-format.mjs';
import { safeUrl } from './live-har-format.mjs';

/** The main frame's document request: its HTTP status, or the reason it never answered. */
export type DocumentOutcome = {
  url: string;
  status: number | null;
  statusText?: string;
  mimeType?: string;
  failed?: string;
};

/** The navigation the monitor started: Page.navigate's refusal and the document it loaded. */
export type NavigationOutcome = {
  error: string | null;
  document: DocumentOutcome | null;
};

/**
 * The final document request of the main frame (the last hop of a redirect chain), or null
 * when the frame is unknown or issued none.
 */
export function mainDocument(
  records: Iterable<NetworkRecord>,
  frameId: string | undefined
): DocumentOutcome | null {
  if (!frameId) return null;
  let last: NetworkRecord | undefined;
  for (const record of records)
    if (
      record.type === 'Document' &&
      record.frameId === frameId &&
      (!last || (record.timestamp ?? 0) >= (last.timestamp ?? 0))
    )
      last = record;
  if (!last) return null;
  const url = safeUrl(last.request?.url ?? last.url);
  if (last.response)
    return {
      url,
      status: last.response.status ?? null,
      ...(last.response.statusText
        ? { statusText: last.response.statusText }
        : {}),
      ...(last.response.mimeType ? { mimeType: last.response.mimeType } : {}),
    };
  return {
    url,
    status: null,
    ...(last.failed
      ? { failed: last.errorText ?? last.blockedReason ?? 'failed' }
      : {}),
  };
}

/** Chrome's own page for a navigation it could not show (DNS failure, empty 4xx/5xx). */
const isChromeErrorPage = (url: string) => url.startsWith('chrome-error:');

/**
 * The page report for `requested`: on Chrome's error page the final URL is the document's
 * and the title is absent; a navigation failure becomes one error row.
 */
export function pageOutcome(
  requested: string,
  {
    navigation,
    page,
  }: {
    navigation: NavigationOutcome;
    page: { url: string | null; title: string | null };
  }
) {
  const errorPage = page.url !== null && isChromeErrorPage(page.url);
  // A server that answered has an HTTP status, which Chrome may still refuse as a navigation
  // (ERR_HTTP_RESPONSE_CODE_FAILURE on an empty 4xx/5xx body); that is not a navigation failure.
  const answered = navigation.document?.status != null;
  const cause = answered
    ? null
    : (navigation.error ?? navigation.document?.failed ?? null);
  const url = safeUrl(navigation.document?.url ?? requested);
  return {
    document: navigation.document,
    finalUrl: errorPage ? url : page.url,
    title: errorPage ? null : page.title,
    navigationErrors: cause
      ? [
          {
            at: new Date().toISOString(),
            kind: 'navigation',
            level: 'error',
            message: `Navigation to ${url} failed: ${cause}`,
            url,
            stack: [],
          },
        ]
      : [],
  };
}
