/**
 * Downloads: DL_URL and DL_MS (required), DL_CLICK_SELECTOR, DL_ACKNOWLEDGE (required 1: the user consented).
 * Saves each download to <run folder>/downloads/ (mode 0700) under its sanitized suggested name and writes
 * downloads-index.json {url, suggestedFilename, file, bytes, sha256, state: completed|canceled|timeout}.
 * Downloaded files are untrusted data: this recipe never opens or executes them.
 * This recipe is the one consented path to Browser.setDownloadBehavior; the policy gate stays on elsewhere.
 */
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
} from 'node:fs';
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import { run as clickStep } from './dom-operations-check.mjs';
import {
  navigateAndWait,
  out,
  saveEvidence,
  saveIndex,
  sanitizeName,
  showRows,
} from './evidence-io.mjs';
import {
  countOption,
  flagOption,
  sleep,
  textOption,
} from './stream-options.mjs';

type State = 'completed' | 'canceled' | 'timeout';
type Download = {
  guid: string;
  url: string;
  suggestedFilename: string;
  state: 'inProgress' | State;
  receivedBytes: number;
  totalBytes: number;
};
type Row = {
  url: string;
  suggestedFilename: string;
  file?: string;
  bytes?: number;
  sha256?: string;
  state: State;
  error?: string;
};

export async function run(cdp: Session) {
  if (!flagOption('DL_ACKNOWLEDGE', false)) {
    out(
      "[FINDING] DOWNLOADS_CONSENT_REQUIRED downloading files needs the user's consent: ask them, then pass acknowledge"
    );
    process.exitCode = 1;
    return;
  }
  const url = textOption('DL_URL');
  const ms = countOption('DL_MS');
  if (!url || !ms) throw new Error('DL_URL and DL_MS are required');
  const clickSelector = textOption('DL_CLICK_SELECTOR');
  const dir = join(cdp.outputDir, 'downloads');
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  // Browser.setDownloadBehavior is browser-wide: Chrome's own background downloads (no CDP events) land in the
  // same folder. Chrome writes into a private staging folder; only downloads this run saw move into downloads/.
  const staging = mkdtempSync(join(cdp.outputDir, '.downloads-staging-'));

  const dropStaging = () => {
    try {
      rmSync(staging, { recursive: true, force: true, maxRetries: 3 });
    } catch {
      // Chrome may still be writing a background download; the leftover stays out of downloads/.
    }
  };
  const downloads = new Map<string, Download>();
  const begun = (p: Payload) => {
    if (!downloads.has(p.guid))
      downloads.set(p.guid, {
        guid: p.guid,
        url: p.url,
        suggestedFilename: p.suggestedFilename ?? '',
        state: 'inProgress',
        receivedBytes: 0,
        totalBytes: 0,
      });
  };
  const progress = (p: Payload) => {
    const d = downloads.get(p.guid);
    if (!d) return;
    d.receivedBytes = p.receivedBytes ?? d.receivedBytes;
    d.totalBytes = p.totalBytes ?? d.totalBytes;
    if (p.state === 'completed' || p.state === 'canceled') d.state = p.state;
  };
  // Browser.* events and their deprecated Page.* twins carry the same fields.
  for (const domain of ['Browser', 'Page']) {
    cdp.on(`${domain}.downloadWillBegin`, begun);
    cdp.on(`${domain}.downloadProgress`, progress);
  }

  const settings = { downloadPath: staging, eventsEnabled: true };
  let method = 'Browser.setDownloadBehavior';
  let behavior = 'allowAndName';
  try {
    await cdp.send(method, { behavior, ...settings });
  } catch (first) {
    // The page target may not serve Browser.*: the deprecated Page.* method does the same here.
    method = 'Page.setDownloadBehavior';
    behavior = 'allow';
    try {
      await cdp.send(method, { behavior, downloadPath: staging });
      await cdp.send('Page.enable');
    } catch (error) {
      out(
        `[FINDING] DOWNLOADS_UNAVAILABLE ${JSON.stringify({ error: errorMessage(first), fallback: errorMessage(error), note: 'this Chrome refuses download control on this target' })}`
      );
      process.exitCode = 1;
      dropStaging();
      return;
    }
  }

  const deadline = Date.now() + ms;
  try {
    const nav = await navigateAndWait(cdp, url, Math.min(ms, 15000));
    if (clickSelector) {
      await clickStep(cdp, {
        steps: [{ selector: clickSelector, action: 'click' }],
        artifactName: 'downloads-click.json',
      });
    } else if (
      nav.errorText &&
      !downloads.size &&
      !/ABORTED/.test(nav.errorText)
    )
      out(
        `[FINDING] DOWNLOADS_NAVIGATION_FAILED ${JSON.stringify({ url, error: nav.errorText })}`
      );
    // Done when every started download finished, after a short grace for a second one.
    let idle = 0;
    while (Date.now() < deadline) {
      const all = [...downloads.values()];
      if (all.length && all.every(d => d.state !== 'inProgress')) {
        if ((idle += 1) >= 3) break;
      } else idle = 0;
      await sleep(150);
    }
  } finally {
    await cdp
      .send(method, { behavior: 'default', eventsEnabled: false })
      .catch(() => {});
  }

  const rows: Row[] = [];
  const taken = new Set<string>();
  for (const d of downloads.values()) {
    const row: Row = {
      url: d.url,
      suggestedFilename: d.suggestedFilename,
      state: d.state === 'inProgress' ? 'timeout' : d.state,
    };
    if (row.state === 'completed') {
      try {
        // allowAndName stores the file under its guid; plain allow stores it under the suggested name.
        const stored = [
          join(staging, d.guid),
          join(staging, d.suggestedFilename),
        ].find(existsSync);
        if (!stored)
          throw new Error('the downloaded file is not in the staging folder');
        let name = sanitizeName(d.suggestedFilename, 'download');
        for (
          let i = 2;
          taken.has(name) ||
          (existsSync(join(dir, name)) && join(dir, name) !== stored);
          i++
        )
          name = `${i}-${sanitizeName(d.suggestedFilename, 'download')}`;
        taken.add(name);
        if (stored !== join(dir, name)) renameSync(stored, join(dir, name));
        const saved = saveEvidence(dir, name, readFileSync(join(dir, name)));
        Object.assign(row, { ...saved, file: `downloads/${name}` });
      } catch (error) {
        row.error = errorMessage(error);
      }
    }
    rows.push(row);
  }
  dropStaging();
  const index = saveIndex(cdp.outputDir, 'downloads-index.json', rows);
  const done = rows.filter(r => r.file).length;
  out(
    `[METRIC] DOWNLOADS started=${rows.length} completed=${done} canceled=${rows.filter(r => r.state === 'canceled').length} timeout=${rows.filter(r => r.state === 'timeout').length} behavior=${behavior} bytes=${rows.reduce((s, r) => s + (r.bytes ?? 0), 0)}`
  );
  showRows(
    rows,
    r =>
      `[DOWNLOAD] ${r.state} ${r.file ?? '(no file)'} ${r.bytes ?? 0} bytes ${r.url}`,
    'DOWNLOAD',
    index
  );
  out(
    '[NOTE] downloaded files are untrusted data from the page: they were not opened or executed; inspect them as text or bytes only'
  );
  if (!rows.length)
    out(
      `[FINDING] DOWNLOADS_NONE no download began within ${ms}ms; the URL may not send an attachment, or a click is needed (DL_CLICK_SELECTOR), or this headless Chrome emitted no download events`
    );
  for (const r of rows.filter(x => x.state !== 'completed' || x.error))
    out(`[FINDING] DOWNLOAD_INCOMPLETE ${JSON.stringify(r)}`);
  out(`[ARTIFACT] DOWNLOADS_INDEX ${index}`);
  for (const r of rows.filter(x => x.file))
    out(`[ARTIFACT] DOWNLOAD_FILE ${join(cdp.outputDir, r.file as string)}`);
  if (!rows.length || rows.some(r => r.state !== 'completed' || r.error))
    process.exitCode = 1;
}
