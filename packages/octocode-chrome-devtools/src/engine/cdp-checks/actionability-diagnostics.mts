import { writeFileSync } from 'fs';
import { join } from 'path';

import type { Session } from '../cdp-connection.mjs';
import { ACTIONABILITY_HELPERS_JS } from '../dom-actionability.mjs';
import { flagArgs } from '../cli-flags.mjs';
import { errorMessage } from '../error-message.mjs';

type NetworkFailure = {
  requestId?: string;
  errorText?: string;
  blockedReason?: string;
  url?: string;
  status?: number;
  statusText?: string;
};
type PageCounts = Record<
  'buttons' | 'inputs' | 'links' | 'forms' | 'scripts' | 'iframes',
  number
>;
type PageSummary = {
  finalUrl: string;
  title: string;
  bodyText: string;
  counts: PageCounts;
  failures: NetworkFailure[];
};

const argv = process.argv.slice(2);
const { getArg } = flagArgs(argv);
const NAVIGATE_URL = getArg('--new-tab', '') || getArg('--url', '');
const WAIT_MS = Math.max(
  500,
  Math.min(15000, Number.parseInt(getArg('--wait-ms', '3000'), 10))
);

function classify({ title, bodyText, counts, failures }: PageSummary) {
  const text = `${title}\n${bodyText}`.toLowerCase();
  const reasons = [];
  if (
    failures.some(f =>
      /403|blocked|captcha|challenge|cloudflare|access denied/i.test(
        `${f.status} ${f.errorText} ${f.url}`
      )
    ) ||
    /captcha|access denied|verify you are human|cloudflare|blocked|unusual traffic|performing security verification|security service to protect against malicious bots/.test(
      text
    )
  )
    reasons.push('blocked');
  if (
    counts.buttons + counts.inputs + counts.links === 0 &&
    bodyText.length < 500
  )
    reasons.push('js-shell');
  if (
    /cookie|consent|privacy choices|gdpr|accept all|manage preferences/.test(
      text
    )
  )
    reasons.push('consent-region');
  if (bodyText.length < 1000 && counts.scripts > 5 && counts.links < 5)
    reasons.push('timing-hydration');
  if (reasons.length === 0) {
    // A page with real interactive elements and real body text isn't a
    // "mismatch" — the generic diagnostic just found nothing wrong. Only
    // blame the selector when the page itself also looks thin/sparse.
    const healthy =
      counts.buttons + counts.inputs + counts.links > 0 &&
      bodyText.length >= 500;
    reasons.push(healthy ? 'no-anomaly-detected' : 'selector-mismatch');
  }
  return reasons;
}

export async function run(cdp: Session) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  const failures: NetworkFailure[] = [];
  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason }) =>
    failures.push({ requestId, errorText, blockedReason })
  );
  cdp.on('Network.responseReceived', ({ response }) => {
    if (response.status >= 400)
      failures.push({
        url: response.url,
        status: response.status,
        statusText: response.statusText,
      });
  });
  if (NAVIGATE_URL && NAVIGATE_URL !== 'about:blank')
    await cdp.send('Page.navigate', { url: NAVIGATE_URL });
  await new Promise(r => setTimeout(r, WAIT_MS));
  const evalResult = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => {
      ${ACTIONABILITY_HELPERS_JS}
      const bodyText = (document.body?.innerText || document.body?.textContent || '').replace(/\\s+/g, ' ').trim();
      const count = s => document.querySelectorAll(s).length;
      const visible = el => isVisible(el);
      const visibleButtons = [...document.querySelectorAll('button,[role=button]')].filter(visible).map(el => ({ text: (el.innerText || el.textContent || el.getAttribute('aria-label') || '').trim(), disabled: isDisabled(el) }));
      const visibleInputs = [...document.querySelectorAll('input,textarea,select')].filter(visible).map(el => ({ tag: el.localName, type: el.type || null, name: el.name || null, placeholder: el.placeholder || null }));
      return { finalUrl: location.href, title: document.title, readyState: document.readyState, bodyText, counts: { buttons: count('button,[role=button]'), inputs: count('input,textarea,select'), links: count('a[href]'), forms: count('form'), scripts: count('script'), iframes: count('iframe') }, visibleButtons, visibleInputs };
    })()`,
  });
  if (evalResult.exceptionDetails || !evalResult.result?.value)
    throw new Error('Diagnostic evaluation unavailable');
  const payload = evalResult.result.value;
  payload.networkFailures = failures;
  payload.classification = classify({
    finalUrl: payload.finalUrl,
    title: payload.title,
    bodyText: payload.bodyText,
    counts: payload.counts,
    failures,
  });
  let screenshotPath: string | null = null;
  try {
    const shot = await cdp.send('Page.captureScreenshot', {
      format: 'png',
      captureBeyondViewport: false,
    });
    screenshotPath = join(cdp.outputDir, 'actionability-diagnostics.png');
    writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'), {
      mode: 0o600,
    });
    payload.screenshot = screenshotPath;
  } catch (error) {
    payload.screenshotError = errorMessage(error);
  }
  const artifact = join(cdp.outputDir, 'actionability-diagnostics.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, {
    mode: 0o600,
  });
  console.log(`[DIAGNOSIS] ${payload.classification.join(',')}`);
  console.log(
    `[METRIC] finalUrl=${payload.finalUrl} title=${JSON.stringify(payload.title)} bodyChars=${payload.bodyText.length} buttons=${payload.counts.buttons} inputs=${payload.counts.inputs} links=${payload.counts.links} failures=${payload.networkFailures.length}`
  );
  if (screenshotPath) console.log(`[SCREENSHOT] ${screenshotPath}`);
  console.log(`[ARTIFACT] ACTIONABILITY_DIAGNOSTICS ${artifact}`);
}
