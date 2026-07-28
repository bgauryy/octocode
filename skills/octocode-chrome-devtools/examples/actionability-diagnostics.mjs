import { writeFileSync } from 'fs';
import { join } from 'path';

const argv = process.argv.slice(2);
const getArg = (flag, def = '') => { const i = argv.indexOf(flag); return i >= 0 && argv[i + 1] ? argv[i + 1] : def; };
const NAVIGATE_URL = getArg('--new-tab', '') || getArg('--url', '');
const WAIT_MS = Math.max(500, Math.min(15000, Number.parseInt(getArg('--wait-ms', '3000'), 10)));

function classify({ finalUrl, title, bodyText, counts, failures }) {
  const text = `${title}\n${bodyText}`.toLowerCase();
  const reasons = [];
  if (failures.some(f => /403|blocked|captcha|challenge|cloudflare|access denied/i.test(`${f.status} ${f.errorText} ${f.url}`)) || /captcha|access denied|verify you are human|cloudflare|blocked|unusual traffic/.test(text)) reasons.push('blocked');
  if ((counts.buttons + counts.inputs + counts.links) === 0 && bodyText.length < 500) reasons.push('js-shell');
  if (/cookie|consent|privacy choices|gdpr|accept all|manage preferences/.test(text)) reasons.push('consent-region');
  if (bodyText.length < 1000 && counts.scripts > 5 && counts.links < 5) reasons.push('timing-hydration');
  if (reasons.length === 0) reasons.push('selector-mismatch');
  return reasons;
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  const failures = [];
  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason }) => failures.push({ requestId, errorText, blockedReason }));
  cdp.on('Network.responseReceived', ({ response }) => { if (response.status >= 400) failures.push({ url: response.url, status: response.status, statusText: response.statusText }); });
  if (NAVIGATE_URL && NAVIGATE_URL !== 'about:blank') await cdp.send('Page.navigate', { url: NAVIGATE_URL });
  await new Promise(r => setTimeout(r, WAIT_MS));
  const evalResult = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => {
      const bodyText = (document.body?.innerText || document.body?.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 5000);
      const count = s => document.querySelectorAll(s).length;
      const visible = el => { const r = el.getBoundingClientRect(); const st = getComputedStyle(el); return !!(r.width && r.height && st.display !== 'none' && st.visibility !== 'hidden' && Number(st.opacity || '1') > 0); };
      const visibleButtons = [...document.querySelectorAll('button,[role=button]')].filter(visible).slice(0, 20).map(el => ({ text: (el.innerText || el.textContent || el.getAttribute('aria-label') || '').trim().slice(0,120), disabled: !!(el.disabled || el.getAttribute('aria-disabled') === 'true') }));
      const visibleInputs = [...document.querySelectorAll('input,textarea,select')].filter(visible).slice(0, 20).map(el => ({ tag: el.localName, type: el.type || null, name: el.name || null, placeholder: el.placeholder || null }));
      return { finalUrl: location.href, title: document.title, readyState: document.readyState, bodyText, counts: { buttons: count('button,[role=button]'), inputs: count('input,textarea,select'), links: count('a[href]'), forms: count('form'), scripts: count('script'), iframes: count('iframe') }, visibleButtons, visibleInputs };
    })()`
  });
  const payload = evalResult.result?.value || { finalUrl: cdp.targetInfo.url, title: '', bodyText: '', counts: { buttons: 0, inputs: 0, links: 0, forms: 0, scripts: 0, iframes: 0 } };
  payload.networkFailures = failures.slice(0, 50);
  payload.classification = classify({ finalUrl: payload.finalUrl, title: payload.title, bodyText: payload.bodyText, counts: payload.counts, failures });
  let screenshotPath = null;
  try {
    const shot = await cdp.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    screenshotPath = join(cdp.outputDir, 'actionability-diagnostics.png');
    writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'), { mode: 0o600 });
    payload.screenshot = screenshotPath;
  } catch (error) { payload.screenshotError = error.message; }
  const artifact = join(cdp.outputDir, 'actionability-diagnostics.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });
  console.log(`[DIAGNOSIS] ${payload.classification.join(',')}`);
  console.log(`[METRIC] finalUrl=${payload.finalUrl} title=${JSON.stringify(payload.title)} bodyChars=${payload.bodyText.length} buttons=${payload.counts.buttons} inputs=${payload.counts.inputs} links=${payload.counts.links} failures=${payload.networkFailures.length}`);
  if (screenshotPath) console.log(`[SCREENSHOT] ${screenshotPath}`);
  console.log(`[ARTIFACT] ACTIONABILITY_DIAGNOSTICS ${artifact}`);
}
