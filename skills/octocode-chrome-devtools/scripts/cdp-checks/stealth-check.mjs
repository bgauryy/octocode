// Apply stealth patches before navigation, then self-test the result.
//
// Usage:
//   node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
//     skills/octocode-chrome-devtools/scripts/cdp-checks/stealth-check.mjs \
//     --port 9222 --new-tab "about:blank" --timeout 30000
//
// Configure the target with STEALTH_CHECK_URL (default: bot.sannysoft.com, a public
// bot-detection self-test page — see references/launch-stealth.md for more test sites).
//
// cdp-sandbox.mjs stages undercover.mjs into <cwd>/.octocode/ (or the global Octocode
// home if the project dir isn't writable) rather than next to this file — a plain
// relative import would look in scripts/cdp-checks/ and fail. Resolve it the same way
// cdp-sandbox.mjs resolved it, then dynamic-import from that path.
import { writeFileSync, existsSync } from 'fs';
import { join, resolve } from 'path';
import { pathToFileURL } from 'url';
import { getOctocodeHome } from '../octocode-config.mjs';

const TARGET_URL = process.env.STEALTH_CHECK_URL ?? 'https://bot.sannysoft.com/';

function resolveHelper(name) {
  const projectPath = resolve(process.cwd(), '.octocode', name);
  return existsSync(projectPath) ? projectPath : resolve(getOctocodeHome(), name);
}

const { applyStealthPatches, verifyStealth } = await import(pathToFileURL(resolveHelper('undercover.mjs')).href);

export async function run(cdp) {
  await cdp.send('Page.enable', {});
  await cdp.send('Runtime.enable', {});

  // Must run before Page.navigate — Page.addScriptToEvaluateOnNewDocument only
  // takes effect on the next navigation in this session.
  await applyStealthPatches(cdp);

  console.log(`[STATUS] Navigating to ${TARGET_URL}`);
  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise((r) => setTimeout(r, 2000));

  const result = await verifyStealth(cdp);
  console.log(`[METRIC] stealth self-test: ${result.passed}/${result.total} passed`);

  // Detector pages such as bot.sannysoft.com mark each probe cell passed/warn/failed.
  const page = (await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => {
      const name = (td) => (td.closest('tr')?.querySelector('td')?.textContent || td.id || '').trim().slice(0, 60);
      const cells = (c) => [...document.querySelectorAll('td.' + c)];
      return { passed: cells('passed').length, warn: cells('warn').length, failed: cells('failed').length,
        failedNames: cells('failed').map(name).slice(0, 20), warnNames: cells('warn').map(name).slice(0, 20) };
    })()`,
  })).result?.value ?? null;
  if (page && page.passed + page.warn + page.failed > 0) {
    console.log(`[METRIC] detector page: passed=${page.passed} warn=${page.warn} failed=${page.failed}`);
    if (page.failed) console.log(`[FINDING] DETECTOR_FAILED ${page.failedNames.join(' | ')}`);
    if (page.warn) console.log(`[FINDING] DETECTOR_WARN ${page.warnNames.join(' | ')}`);
  }

  if (cdp.outputDir) {
    const outPath = join(cdp.outputDir, 'stealth-check.json');
    writeFileSync(outPath, JSON.stringify({ targetUrl: TARGET_URL, ...result, detectorPage: page }, null, 2));
    console.log(`[ARTIFACT] ${outPath}`);
  }

  if (page?.failed > 0) {
    console.log('[FINDING] Detector page flags leaks the self-test missed; see DETECTOR_FAILED above.');
  } else if (result.failed > 0) {
    console.log('[FINDING] Stealth self-test has failures — inspect [FINDING] STEALTH_FAIL lines above for which signals leaked.');
  } else {
    console.log('[ACTION] Stealth posture clean — safe to proceed with scraping this target.');
  }
}
