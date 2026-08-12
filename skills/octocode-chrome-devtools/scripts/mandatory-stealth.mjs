/**
 * Mandatory stealth gate for every CDP run (unless CDP_NO_STEALTH=1).
 */
import { applyStealthPatches, verifyStealth } from './undercover.mjs';

export function stealthEnabled() {
  const v = process.env.CDP_NO_STEALTH;
  return v !== '1' && v !== 'true';
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

  let origin = opts.origin;
  if (!origin && opts.navigateUrl) {
    try { origin = new URL(opts.navigateUrl).origin; } catch { /* ignore */ }
  }
  if (!origin && cdp.targetInfo?.url && !isAboutOrDataUrl(cdp.targetInfo.url)) {
    try { origin = new URL(cdp.targetInfo.url).origin; } catch { /* ignore */ }
  }

  await applyStealthPatches(cdp, origin ? { origin } : {});
  cdp.stealthApplied = true;
  console.log('[INJECT] Stealth patches applied (mandatory gate)');

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
