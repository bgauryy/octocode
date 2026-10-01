import { writeFileSync, readFileSync } from 'fs';
import { join } from 'path';

// Screenshot of the attached tab: viewport (default), full page, or one element.
// Env:
//   SHOT_FULL=1          full scrollable page (height capped at 8000 CSS px)
//   SHOT_SELECTOR=<css>  clip to the first matching element (scrolled into view)
//   SHOT_FORMAT          jpeg (default, smaller) | png
//   SHOT_QUALITY         jpeg quality 30-95 (default 70)
//   SHOT_SCALE           0.25-1 (default 1); 0.5 quarters the pixels for cheap visual checks
//   SHOT_ANNOTATE=1      draw the latest page-snapshot refs (eN boxes) on the image; removed after capture

const FULL = process.env.SHOT_FULL === '1';
const SELECTOR = process.env.SHOT_SELECTOR || '';
const FORMAT = process.env.SHOT_FORMAT === 'png' ? 'png' : 'jpeg';
const QUALITY = Math.max(30, Math.min(95, Number.parseInt(process.env.SHOT_QUALITY ?? '70', 10) || 70));
const MAX_HEIGHT = 8000;
const SCALE = Math.max(0.25, Math.min(1, Number.parseFloat(process.env.SHOT_SCALE ?? '1') || 1));
const ANNOTATE = process.env.SHOT_ANNOTATE === '1';
const MAX_LABELS = 250;

// Box every snapshot ref that is on screen (page coordinates, so full-page shots work too).
async function annotate(cdp) {
  let refs;
  try {
    const map = JSON.parse(readFileSync(cdp.resourcesFile, 'utf8'));
    refs = JSON.parse(readFileSync(map.resources['page-snapshot'].artifactPath, 'utf8')).refs ?? {};
  } catch {
    console.log('[FINDING] SHOT_NO_SNAPSHOT run page-snapshot.mjs first; SHOT_ANNOTATE draws its refs');
    return 0;
  }
  const { cssVisualViewport: v, cssContentSize: c } = await cdp.send('Page.getLayoutMetrics');
  const bottom = FULL ? Math.min(c.height, MAX_HEIGHT) : v.pageY + v.clientHeight;
  const top = FULL ? 0 : v.pageY;
  const boxes = [];
  for (const [ref, entry] of Object.entries(refs)) {
    if (boxes.length >= MAX_LABELS) break;
    const q = (await cdp.send('DOM.getContentQuads', { backendNodeId: entry.backendDOMNodeId }).catch(() => null))?.quads?.[0];
    if (!q) continue;
    const xs = [q[0], q[2], q[4], q[6]].map((x) => x + v.pageX);
    const ys = [q[1], q[3], q[5], q[7]].map((y) => y + v.pageY);
    const box = { ref, x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
    if (box.w < 2 || box.h < 2 || box.y + box.h < top || box.y > bottom) continue;
    boxes.push(box);
  }
  await cdp.send('Runtime.evaluate', {
    expression: `(() => {
      const host = document.createElement('div');
      host.id = '__octo_marks';
      host.style.cssText = 'position:absolute;left:0;top:0;width:0;height:0;z-index:2147483647;pointer-events:none';
      for (const b of ${JSON.stringify(boxes)}) {
        const d = document.createElement('div');
        d.style.cssText = 'position:absolute;box-sizing:border-box;border:2px solid #e3008c;left:' + b.x + 'px;top:' + b.y + 'px;width:' + b.w + 'px;height:' + b.h + 'px';
        const t = document.createElement('span');
        t.textContent = b.ref;
        t.style.cssText = 'position:absolute;left:-2px;top:-2px;transform:translateY(-100%);background:#e3008c;color:#fff;font:bold 11px/13px monospace;padding:0 2px;white-space:nowrap';
        d.appendChild(t);
        host.appendChild(d);
      }
      document.documentElement.appendChild(host);
    })()`,
  });
  return boxes.length;
}

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

  const labels = ANNOTATE ? await annotate(cdp) : 0;
  let data;
  try {
    ({ data } = await cdp.send('Page.captureScreenshot', {
      format: FORMAT,
      ...(FORMAT === 'jpeg' ? { quality: QUALITY } : {}),
      ...(clip ? { clip, captureBeyondViewport: true } : {}),
    }));
  } finally {
    if (ANNOTATE) await cdp.send('Runtime.evaluate', { expression: "document.getElementById('__octo_marks')?.remove()" }).catch(() => {});
  }
  const buf = Buffer.from(data, 'base64');
  const file = join(cdp.outputDir, `screenshot${SELECTOR ? '-element' : FULL ? '-full' : ''}.${FORMAT === 'png' ? 'png' : 'jpg'}`);
  writeFileSync(file, buf, { mode: 0o600 });
  const title = await evaluate('document.title');
  console.log(`[METRIC] SCREENSHOT bytes=${buf.length} mode=${SELECTOR ? 'element' : FULL ? 'full' : 'viewport'}${clip ? ` size=${Math.round(clip.width * clip.scale)}x${Math.round(clip.height * clip.scale)}` : ''} title="${String(title ?? '').slice(0, 80)}"${ANNOTATE ? ` labels=${labels}` : ''}`);
  console.log(`[SCREENSHOT] ${file}`);
}
