/**
 * Page archive: ARCHIVE_URL; one MHTML file (page.mhtml) from Page.captureSnapshot, plus archive-index.json.
 */
import type { Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import {
  navigateAndWait,
  out,
  saveEvidence,
  saveIndex,
} from './evidence-io.mjs';
import { textOption } from './stream-options.mjs';

export async function run(cdp: Session) {
  const url = textOption('ARCHIVE_URL');
  if (url) {
    const nav = await navigateAndWait(cdp, url);
    if (nav.errorText) {
      out(
        `[FINDING] PAGE_ARCHIVE_NAVIGATION_FAILED ${JSON.stringify({ url, error: nav.errorText })}`
      );
      process.exitCode = 1;
      return;
    }
  }
  let data: string;
  try {
    ({ data } = await cdp.send('Page.captureSnapshot', { format: 'mhtml' }));
    if (typeof data !== 'string' || !data)
      throw new Error('Chrome returned an empty snapshot');
  } catch (error) {
    out(
      `[FINDING] PAGE_ARCHIVE_UNAVAILABLE ${JSON.stringify({ error: errorMessage(error), note: 'Page.captureSnapshot is experimental; some headless or older Chrome builds refuse it, and only a page target can be archived' })}`
    );
    process.exitCode = 1;
    return;
  }
  const pageUrl = String(
    (await cdp.send('Page.getFrameTree')).frameTree?.frame?.url ?? ''
  );
  const saved = saveEvidence(cdp.outputDir, 'page.mhtml', data);
  const index = saveIndex(cdp.outputDir, 'archive-index.json', [
    { ...saved, url: pageUrl },
  ]);
  out(
    `[METRIC] PAGE_ARCHIVE bytes=${saved.bytes} sha256=${saved.sha256} url=${pageUrl}`
  );
  out(`[ARTIFACT] PAGE_ARCHIVE ${cdp.outputDir}/${saved.file}`);
  out(`[ARTIFACT] PAGE_ARCHIVE_INDEX ${index}`);
}
