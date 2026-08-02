// Apply stealth patches before navigation, then self-test the result.
//
// Usage:
//   node skills/octocode-chrome-devtools/scripts/cdp-sandbox.mjs \
//     skills/octocode-chrome-devtools/examples/stealth-check.mjs \
//     --port 9222 --new-tab "about:blank" --timeout 30000
//
// Configure the target with STEALTH_CHECK_URL (default: bot.sannysoft.com, a public
// bot-detection self-test page — see references/intents-environment.md for more test sites).
//
// cdp-sandbox.mjs stages undercover.mjs into <cwd>/.octocode/ (or the global Octocode
// home if the project dir isn't writable) rather than next to this file — a plain
// relative import would look in examples/ and fail. Resolve it the same way
// cdp-sandbox.mjs resolved it, then dynamic-import from that path.
import { writeFileSync, existsSync } from 'fs';
import { join, resolve } from 'path';
import { pathToFileURL } from 'url';
import { getOctocodeHome } from '@octocodeai/config';

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

  if (cdp.outputDir) {
    const outPath = join(cdp.outputDir, 'stealth-check.json');
    writeFileSync(outPath, JSON.stringify({ targetUrl: TARGET_URL, ...result }, null, 2));
    console.log(`[ARTIFACT] ${outPath}`);
  }

  if (result.failed > 0) {
    console.log('[FINDING] Stealth self-test has failures — inspect [FINDING] STEALTH_FAIL lines above for which signals leaked.');
  } else {
    console.log('[ACTION] Stealth posture clean — safe to proceed with scraping this target.');
  }
}
