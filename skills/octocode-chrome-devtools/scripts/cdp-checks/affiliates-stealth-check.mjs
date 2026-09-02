// Stealth-mandatory check: Walmart Affiliate Program landing (public marketing site).
// CDP runner applies stealth before navigation when using --new-tab <https url>.
//
//   node skills/octocode-chrome-devtools/scripts/open-browser.mjs --headless --port 9222 --url about:blank
//   node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
//     skills/octocode-chrome-devtools/scripts/cdp-checks/affiliates-stealth-check.mjs \
//     --port 9222 --new-tab "https://affiliates.walmart.com/" --timeout 60000 --keep-tab
//
import { writeFileSync } from 'fs';
import { join } from 'path';

const TARGET = process.env.AFFILIATES_CHECK_URL ?? 'https://affiliates.walmart.com/';

export async function run(cdp) {
  if (!cdp.stealthApplied) {
    throw new Error('[STEALTH_GATE] mandatory stealth was not applied by cdp-runner — do not use --no-stealth');
  }

  await cdp.send('Network.enable', {});

  let reqCount = 0;
  cdp.on('Network.requestWillBeSent', () => { reqCount += 1; });

  const page = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: `({
      title: document.title,
      h1: document.querySelector('h1')?.innerText?.trim()?.slice(0, 120) || '',
      href: location.href,
      signupLinks: [...document.querySelectorAll('a')].filter(a => /sign up|apply|join/i.test(a.innerText||'')).slice(0,5).map(a => ({ text: (a.innerText||'').trim().slice(0,60), href: a.href }))
    })`,
  });
  const info = page.result?.value || {};
  console.log(`[METRIC] PAGE title="${info.title}"`);
  console.log(`[METRIC] PAGE h1="${info.h1}"`);
  console.log(`[METRIC] NETWORK requestsDuringSession=${reqCount}`);
  console.log(`[FINDING] signup CTAs=${JSON.stringify(info.signupLinks)}`);

  let cookies = [];
  try { cookies = (await cdp.send('Network.getAllCookies')).cookies || []; } catch {}
  const storage = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const safeKeys = obj => { try { return Object.keys(obj); } catch { return []; } };
      return { url: location.href, localStorageKeys: safeKeys(localStorage), sessionStorageKeys: safeKeys(sessionStorage) };
    })()`,
  });
  const cookieRows = cookies.map(c => ({ name: c.name, domain: c.domain, secure: c.secure }));
  const payload = {
    target: TARGET,
    stealthVerify: cdp.stealthVerify ?? null,
    page: info,
    requestsDuringSession: reqCount,
    cookies: { count: cookieRows.length, rows: cookieRows },
    storage: storage.result?.value,
  };
  const out = join(cdp.outputDir, 'affiliates-stealth-check.json');
  writeFileSync(out, JSON.stringify(payload, null, 2));
  console.log(`[ARTIFACT] ${out}`);
  console.log(`[METRIC] COOKIES count=${cookieRows.length}`);
  console.log(`[METRIC] STORAGE localStorageKeys=${payload.storage?.localStorageKeys?.length ?? 0}`);

  if (!/Walmart Affiliate/i.test(info.title || '') && !/Become a Walmart Affiliate/i.test(info.h1 || '')) {
    throw new Error('affiliate landing check: title/h1 mismatch (bot wall or redirect)');
  }
  if (!info.href?.includes('affiliates.walmart.com')) {
    throw new Error(`affiliate landing check: unexpected href ${info.href}`);
  }
}
