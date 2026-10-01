import { writeFileSync } from 'fs';
import { join } from 'path';

// Screenshot of the attached tab: viewport (default), full page, or one element.
// Env:
//   SHOT_FULL=1          full scrollable page (height capped at 8000 CSS px)
//   SHOT_SELECTOR=<css>  clip to the first matching element (scrolled into view)
//   SHOT_FORMAT          jpeg (default, smaller) | png
//   SHOT_QUALITY         jpeg quality 30-95 (default 70)
//   SHOT_SCALE           0.25-1 (default 1); 0.5 quarters the pixels for cheap visual checks

const FULL = process.env.SHOT_FULL === '1';
const SELECTOR = process.env.SHOT_SELECTOR || '';
const FORMAT = process.env.SHOT_FORMAT === 'png' ? 'png' : 'jpeg';
const QUALITY = Math.max(30, Math.min(95, Number.parseInt(process.env.SHOT_QUALITY ?? '70', 10) || 70));
const MAX_HEIGHT = 8000;
const SCALE = Math.max(0.25, Math.min(1, Number.parseFloat(process.env.SHOT_SCALE ?? '1') || 1));

export async function run(cdp) {
  await cdp.send('Page.enable');
  const evaluate = async (expression) => (await cdp.send('Runtime.evaluate', { expression, returnByValue: true })).result?.value;
  for (let i = 0; i < 20 && (await evaluate('document.readyState')) !== 'complete'; i++) await new Promise((r) => setTimeout(r, 250));

  let clip;
  if (SELECTOR) {
    const box = await evaluate(`(() => { const el = document.querySelector(${JSON.stringify(SELECTOR)}); if (!el) return null;
      el.scrollIntoView({ block: 'center' }); const r = el.getBoundingClientRect();
      return { x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height }; })()`);
    if (!box || !box.width || !box.height) {
      console.log(`[FINDING] SHOT_SELECTOR_MISSING ${SELECTOR} not found or zero-size`);
      return;
    }
    clip = { ...box, height: Math.min(box.height, MAX_HEIGHT), scale: SCALE };
  } else if (FULL) {
    const { cssContentSize, cssLayoutViewport } = await cdp.send('Page.getLayoutMetrics');
    const width = Math.ceil(cssLayoutViewport?.clientWidth ?? cssContentSize.width);
    const height = Math.min(Math.ceil(cssContentSize.height), MAX_HEIGHT);
    if (cssContentSize.height > MAX_HEIGHT) console.log(`[FINDING] SHOT_TRUNCATED page is ${Math.ceil(cssContentSize.height)}px tall; captured ${MAX_HEIGHT}px`);
    clip = { x: 0, y: 0, width, height, scale: SCALE };
  } else if (SCALE < 1) {
    const { cssVisualViewport: v } = await cdp.send('Page.getLayoutMetrics');
    clip = { x: v.pageX, y: v.pageY, width: v.clientWidth, height: v.clientHeight, scale: SCALE };
  }

  const { data } = await cdp.send('Page.captureScreenshot', {
    format: FORMAT,
    ...(FORMAT === 'jpeg' ? { quality: QUALITY } : {}),
    ...(clip ? { clip, captureBeyondViewport: true } : {}),
  });
  const buf = Buffer.from(data, 'base64');
  const file = join(cdp.outputDir, `screenshot${SELECTOR ? '-element' : FULL ? '-full' : ''}.${FORMAT === 'png' ? 'png' : 'jpg'}`);
  writeFileSync(file, buf, { mode: 0o600 });
  const title = await evaluate('document.title');
  console.log(`[METRIC] SCREENSHOT bytes=${buf.length} mode=${SELECTOR ? 'element' : FULL ? 'full' : 'viewport'}${clip ? ` size=${Math.round(clip.width * clip.scale)}x${Math.round(clip.height * clip.scale)}` : ''} title="${String(title ?? '').slice(0, 80)}"`);
  console.log(`[SCREENSHOT] ${file}`);
}
