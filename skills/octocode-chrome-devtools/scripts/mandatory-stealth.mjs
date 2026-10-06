/**
 * Opt-in browser emulation for CDP runs. Native browser settings are the default.
 */
import { applyStealthPatches, verifyStealth } from './undercover.mjs';

export function stealthEnabled() {
  const v = process.env.CDP_NO_STEALTH;
  return process.env.CDP_STEALTH === '1' && v !== '1' && v !== 'true';
}

export function isAboutOrDataUrl(url) {
  if (!url) return true;
  return url === 'about:blank' || url.startsWith('about:') || url.startsWith('data:');
}

export async function applyMandatoryStealth(cdp, opts = {}) {
  if (!stealthEnabled()) {
    console.log('[FINDING] STEALTH_SKIPPED CDP_NO_STEALTH is set');
    return { skipped: true };
  }
  if (cdp.stealthApplied) {
    return cdp.stealthVerify ?? { passed: 0, failed: 0, total: 0, reused: true };
  }
  await cdp.send('Page.enable', {}).catch(() => {});
  await cdp.send('Runtime.enable', {}).catch(() => {});

  await applyStealthPatches(cdp);
  cdp.stealthApplied = true;
  console.log('[INJECT] Stealth patches applied (opt-in)');

  // Kept-tab follow-up steps: reloading would erase form/SPA state. The current
  // document was loaded under the previous run's patches, so skip reload+verify.
  if (process.env.CDP_STEALTH_NO_RELOAD === '1' && !opts.navigateUrl) {
    console.log('[FINDING] STEALTH_RELOAD_SKIPPED patches apply on next navigation; page state kept');
    return { skippedReload: true };
  }

  // Patches register on new document; reload so verify runs on injected JS world.
  try {
    await cdp.send('Page.reload', { ignoreCache: false });
    await new Promise((r) => setTimeout(r, 600));
  } catch {
    await cdp.send('Page.navigate', { url: cdp.targetInfo?.url || 'about:blank' });
    await new Promise((r) => setTimeout(r, 600));
  }

  if (process.env.CDP_SKIP_STEALTH_VERIFY === '1') {
    return { skippedVerify: true };
  }

  const result = await verifyStealth(cdp);
  console.log(`[METRIC] stealth self-test: ${result.passed}/${result.total} passed`);
  cdp.stealthVerify = result;

  if (result.failed > 0 && process.env.CDP_STEALTH_ALLOW_FAIL !== '1') {
    const err = new Error(`[STEALTH_GATE] ${result.failed}/${result.total} stealth checks failed`);
    err.stealthResult = result;
    throw err;
  }
  return result;
}

/** Apply stealth (if needed), navigate, brief settle. */
export async function ensureStealthNavigate(cdp, url, { waitMs = 2500 } = {}) {
  await applyMandatoryStealth(cdp, { navigateUrl: url });
  const current = cdp.targetInfo?.url ?? '';
  if (!current.includes(url.replace(/^https?:\/\//, '').split('/')[0])) {
    await cdp.send('Page.navigate', { url });
    await new Promise((r) => setTimeout(r, waitMs));
    cdp.targetInfo = { ...cdp.targetInfo, url };
  } else if (!current.startsWith(url.split('?')[0]) && url.startsWith('http')) {
    await cdp.send('Page.navigate', { url });
    await new Promise((r) => setTimeout(r, waitMs));
    cdp.targetInfo = { ...cdp.targetInfo, url };
  }
}
