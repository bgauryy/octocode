/**
 * Opt-in browser emulation for CDP runs. Native browser settings are the default.
 */
import type { Payload } from './cdp-connection.mjs';
import type { StealthSession } from './stealth-session.mjs';
import { applyStealthPatches, verifyStealth } from './undercover.mjs';

export function stealthEnabled() {
  const v = process.env.CDP_NO_STEALTH;
  return process.env.CDP_STEALTH === '1' && v !== '1' && v !== 'true';
}

export function isAboutOrDataUrl(url: string | null | undefined) {
  if (!url) return true;
  return (
    url === 'about:blank' || url.startsWith('about:') || url.startsWith('data:')
  );
}

export async function applyMandatoryStealth(
  cdp: StealthSession,
  opts: { navigateUrl?: string } = {}
) {
  if (!stealthEnabled()) {
    console.log('[FINDING] STEALTH_SKIPPED CDP_NO_STEALTH is set');
    return { skipped: true };
  }
  if (cdp.stealthApplied) {
    return (
      cdp.stealthVerify ?? { passed: 0, failed: 0, total: 0, reused: true }
    );
  }
  await cdp.send('Page.enable', {}).catch(() => {});
  await cdp.send('Runtime.enable', {}).catch(() => {});

  await applyStealthPatches(cdp);
  cdp.stealthApplied = true;

  // Overrides reach every later request at once. A pending navigation (new tab) needs no
  // reload; an attached page reloads so its document loads under them, unless the caller
  // keeps page state, in which case only the current document's load headers stay native.
  const href = await cdp
    .send('Runtime.evaluate', {
      expression: 'location.href',
      returnByValue: true,
    })
    .then((r: Payload) => String(r?.result?.value ?? ''))
    .catch(() => '');
  if (opts.navigateUrl || (href && isAboutOrDataUrl(href))) {
    // The runner navigates next, or the document was never fetched.
  } else if (process.env.CDP_STEALTH_NO_RELOAD === '1') {
    console.log(
      '[FINDING] STEALTH_RELOAD_SKIPPED overrides apply to later requests; the current document kept its load headers'
    );
  } else {
    await reloadAndWait(cdp);
  }

  if (process.env.CDP_SKIP_STEALTH_VERIFY === '1') {
    return { skippedVerify: true };
  }

  const result = await verifyStealth(cdp);
  console.log(
    `[METRIC] stealth self-test: ${result.passed}/${result.total} passed`
  );
  cdp.stealthVerify = result;

  if (result.failed > 0 && process.env.CDP_STEALTH_ALLOW_FAIL !== '1') {
    const err = new Error(
      `[STEALTH_GATE] ${result.failed}/${result.total} stealth checks failed`
    );
    Object.assign(err, { stealthResult: result });
    throw err;
  }
  return result;
}

async function reloadAndWait(cdp: StealthSession, timeoutMs = 15000) {
  const { promise: loaded, resolve: done } = Promise.withResolvers<void>();
  const onLoad = (_params: Payload, meta?: { sessionId?: string }) => {
    if (!meta?.sessionId) done();
  };
  cdp.on('Page.loadEventFired', onLoad);
  const timer = setTimeout(() => done(), timeoutMs);
  try {
    await cdp.send('Page.reload', { ignoreCache: false });
    await loaded;
  } finally {
    clearTimeout(timer);
    cdp.off('Page.loadEventFired', onLoad);
  }
}
