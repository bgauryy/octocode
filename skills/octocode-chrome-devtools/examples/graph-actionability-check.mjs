import { readFileSync, writeFileSync } from 'fs';
import { join, resolve } from 'path';

const argv = process.argv.slice(2);
const getArg = (flag, def = '') => { const i = argv.indexOf(flag); return i >= 0 && argv[i + 1] ? argv[i + 1] : def; };
const GRAPH = getArg('--graph', '');
const NAVIGATE_URL = getArg('--new-tab', '') || getArg('--url', '');
const SELECTORS = getArg('--selectors', 'button, [role="button"], input, textarea, select, a[href]');
const LIMIT = Math.max(1, Math.min(100, Number.parseInt(getArg('--limit', '25'), 10)));

function selectorsFromGraph(graphPath) {
  if (!graphPath) return [];
  try {
    const graph = JSON.parse(readFileSync(resolve(graphPath), 'utf8'));
    return (graph.nodes || [])
      .filter(n => ['form', 'input', 'button', 'pagination'].includes(n.kind))
      .map(n => n.selector)
      .filter(Boolean);
  } catch {
    return [];
  }
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('DOM.enable');
  await cdp.send('Accessibility.enable');
  await cdp.send('Page.enable');
  if (NAVIGATE_URL && NAVIGATE_URL !== 'about:blank') {
    await cdp.send('Page.navigate', { url: NAVIGATE_URL });
    await new Promise(r => setTimeout(r, 2500));
  }
  const graphSelectors = selectorsFromGraph(GRAPH);
  const selectorList = [...new Set([...graphSelectors, ...SELECTORS.split(',').map(s => s.trim()).filter(Boolean)])].slice(0, LIMIT);
  const result = await cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const selectors = ${JSON.stringify(selectorList)};
      const sleep = ms => new Promise(r => setTimeout(r, ms));
      const short = v => String(v ?? '').replace(/\s+/g, ' ').trim().slice(0, 160);
      function nameOf(el) {
        const labelledBy = el.getAttribute('aria-labelledby');
        if (labelledBy) {
          const text = labelledBy.split(/\s+/).map(id => document.getElementById(id)?.textContent ?? '').join(' ');
          if (short(text)) return short(text);
        }
        return short(el.getAttribute('aria-label') || el.labels?.[0]?.textContent || el.alt || el.title || el.innerText || el.textContent || el.value);
      }
      function pathOf(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        const testId = el.getAttribute('data-testid');
        if (testId) return '[data-testid="' + testId.replace(/"/g, '\\"') + '"]';
        const name = el.getAttribute('name');
        if (name) return el.localName + '[name="' + name.replace(/"/g, '\\"') + '"]';
        return el.localName;
      }
      const rows = [];
      for (const selector of selectors) {
        for (const el of [...document.querySelectorAll(selector)].slice(0, 20)) {
          el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
          const r1 = el.getBoundingClientRect();
          await sleep(100);
          const r2 = el.getBoundingClientRect();
          const style = getComputedStyle(el);
          const x = r2.left + r2.width / 2, y = r2.top + r2.height / 2;
          const hit = document.elementFromPoint(x, y);
          const stable = Math.abs(r1.x-r2.x)+Math.abs(r1.y-r2.y)+Math.abs(r1.width-r2.width)+Math.abs(r1.height-r2.height) < 1;
          const visible = Boolean(r2.width && r2.height && style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity || '1') > 0);
          const disabled = Boolean(el.disabled || el.getAttribute('aria-disabled') === 'true' || el.closest('[inert]'));
          const covered = Boolean(hit && hit !== el && !el.contains(hit));
          rows.push({ selector, path: pathOf(el), tag: el.localName, role: el.getAttribute('role') || el.localName, name: nameOf(el), text: short(el.innerText || el.textContent), href: el.href || null, type: el.getAttribute('type'), visible, disabled, stable, covered, canOperate: visible && !disabled && stable && !covered, bbox: { x: Math.round(r2.x), y: Math.round(r2.y), width: Math.round(r2.width), height: Math.round(r2.height) } });
        }
      }
      return { url: location.href, rows };
    })()`
  });
  const payload = result.result?.value ?? { rows: [] };
  const artifact = join(cdp.outputDir, 'graph-actionability.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });
  const rows = payload.rows || [];
  console.log(`[METRIC] ACTIONABILITY rows=${rows.length} operable=${rows.filter(r => r.canOperate).length}`);
  for (const row of rows.slice(0, 10)) console.log(`[ACTIONABILITY] ${JSON.stringify(row)}`);
  console.log(`[ARTIFACT] ACTIONABILITY ${artifact}`);
}
