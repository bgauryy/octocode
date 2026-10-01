/**
 * Smart performance measure: navigation/paint/LCP/CLS/long-tasks/resources.
 * Hermetic: navigates a local fixture. Live: set MEASURE_URL or attach to current tab
 * (MEASURE_EXISTING=1 skips navigate when page is already loaded).
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

const SLOW_RESOURCE_MS = Number(process.env.PERF_SLOW_RESOURCE_MS || 500);
const FIXTURE = `data:text/html,${encodeURIComponent(`<!doctype html>
<html><head><title>perf-fixture</title>
<style>body{font:16px sans-serif}.hero{font-size:32px;margin:24px}</style>
</head><body>
<div class="hero" id="hero">Performance fixture</div>
<img id="img" width="120" height="40" alt="x"
  src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"/>
<script>
  // Force a measurable long task (>50ms)
  const t0 = performance.now();
  while (performance.now() - t0 < 60) {}
  // Mark custom timing
  performance.mark('fixture-ready');
  performance.measure('fixture-boot', { start: 0, end: performance.now() });
  // Late banner pushes content down: a measurable layout shift.
  setTimeout(() => { const b = document.createElement('div'); b.style.height = '300px'; b.textContent = 'late banner'; document.body.prepend(b); }, 300);
</script>
<p>Paragraph that will shift.</p><p>Another paragraph that will shift.</p>
</body></html>`)}`;

async function collect(cdp) {
  return cdp.send('Runtime.evaluate', {
    awaitPromise: true,
    returnByValue: true,
    expression: `(async () => {
      const nav = performance.getEntriesByType('navigation')[0];
      const paints = Object.fromEntries(
        performance.getEntriesByType('paint').map(p => [p.name, Math.round(p.startTime)])
      );
      const resources = performance.getEntriesByType('resource').map(r => ({
        name: String(r.name).slice(0, 300),
        initiatorType: r.initiatorType,
        duration: Math.round(r.duration),
        transferSize: r.transferSize || 0,
        encodedBodySize: r.encodedBodySize || 0,
      })).sort((a,b) => b.duration - a.duration).slice(0, 100);

      const longTasks = [];
      try {
        if (!globalThis.__octocodeLtObs) {
          globalThis.__octocodeLongTasks = globalThis.__octocodeLongTasks || [];
          if ('PerformanceObserver' in globalThis) {
            globalThis.__octocodeLtObs = new PerformanceObserver(list => {
              for (const e of list.getEntries()) {
                globalThis.__octocodeLongTasks.push({
                  startTime: Math.round(e.startTime),
                  duration: Math.round(e.duration),
                  name: e.name,
                });
              }
            });
            globalThis.__octocodeLtObs.observe({ type: 'longtask', buffered: true });
          }
        }
      } catch {}
      for (const e of (globalThis.__octocodeLongTasks || [])) longTasks.push(e);

      // LCP, layout-shift and event timing are not in the performance timeline buffer:
      // getEntriesByType() returns [] for them. Only a buffered observer sees them.
      const buffered = (type) => new Promise((resolve) => {
        const got = [];
        let obs;
        try {
          obs = new PerformanceObserver((list) => { got.push(...list.getEntries()); });
          obs.observe({ type, buffered: true, ...(type === 'event' ? { durationThreshold: 16 } : {}) });
        } catch { resolve([]); return; }
        setTimeout(() => { got.push(...obs.takeRecords()); obs.disconnect(); resolve(got); }, 150);
      });
      const describe = (el) => {
        if (!el) return null;
        const id = el.id ? '#' + el.id : '';
        const cls = typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\\s+/)[0] : '';
        return (el.tagName.toLowerCase() + id + cls).slice(0, 80);
      };
      const [lcpEntries, shifts, events] = await Promise.all([
        buffered('largest-contentful-paint'), buffered('layout-shift'), buffered('event'),
      ]);
      let lcp = null;
      if (lcpEntries.length) {
        const last = lcpEntries[lcpEntries.length - 1];
        lcp = { startTime: Math.round(last.startTime || last.renderTime || last.loadTime), size: last.size || 0,
          element: describe(last.element), url: last.url ? String(last.url).slice(0, 200) : null };
      }
      // CLS = largest session window (gap <1s, window <=5s), excluding shifts after input.
      let cls = null;
      const real = shifts.filter(e => !e.hadRecentInput);
      if (real.length || shifts.length) {
        let best = 0, cur = 0, first = 0, prev = 0;
        for (const e of real) {
          if (cur && (e.startTime - prev > 1000 || e.startTime - first > 5000)) { cur = 0; }
          if (!cur) first = e.startTime;
          cur += e.value; prev = e.startTime; best = Math.max(best, cur);
        }
        cls = Number(best.toFixed(4));
      } else {
        cls = 0;
      }
      // Headless paints lazily, so e.duration (until next paint) is inflated by idle frames;
      // score input delay + handler time, keep the presented duration as context.
      const interactions = events.filter(e => e.interactionId);
      const inp = interactions.length ? interactions.reduce((m, e) => Math.max(m, Math.round(e.processingEnd - e.startTime)), 0) : null;
      const inpPresented = interactions.length ? interactions.reduce((m, e) => Math.max(m, Math.round(e.duration)), 0) : null;
      const visibility = document.visibilityState;

      const measures = performance.getEntriesByType('measure').map(m => ({
        name: m.name, duration: Math.round(m.duration), startTime: Math.round(m.startTime),
      })).slice(0, 20);

      return {
        url: location.href,
        title: document.title,
        navigation: nav ? {
          type: nav.type,
          domContentLoaded: Math.round(nav.domContentLoadedEventEnd),
          load: Math.round(nav.loadEventEnd),
          responseStart: Math.round(nav.responseStart),
          responseEnd: Math.round(nav.responseEnd),
          transferSize: nav.transferSize || 0,
          encodedBodySize: nav.encodedBodySize || 0,
        } : null,
        paints,
        fcp: paints['first-contentful-paint'] ?? paints['first-paint'] ?? null,
        lcp,
        cls,
        inp,
        inpPresented,
        visibility,
        longTasks: longTasks.slice(-50),
        resources,
        measures,
        memoryUsed: performance.memory?.usedJSHeapSize ?? null,
        now: Math.round(performance.now()),
      };
    })()`,
  });
}

function score(snap, slowMs) {
  const findings = [];
  const fcp = snap.fcp;
  const lcp = snap.lcp?.startTime ?? null;
  const longTaskTotal = (snap.longTasks || []).reduce((s, t) => s + (t.duration || 0), 0);
  const slowResources = (snap.resources || []).filter(r => r.duration >= slowMs);

  if (fcp != null && fcp > 2000) findings.push({ code: 'SLOW_FCP', ms: fcp });
  if (lcp != null && lcp > 2500) findings.push({ code: 'SLOW_LCP', ms: lcp });
  if ((snap.cls ?? 0) > 0.1) findings.push({ code: 'HIGH_CLS', cls: snap.cls });
  if ((snap.inp ?? 0) > 200) findings.push({ code: 'SLOW_INP', ms: snap.inp });
  if (longTaskTotal > 100) findings.push({ code: 'LONG_TASKS', totalMs: longTaskTotal, count: snap.longTasks.length });
  if (slowResources.length) findings.push({ code: 'SLOW_RESOURCES', count: slowResources.length, top: slowResources.slice(0, 5) });

  // Health score 0–100 (heuristic, not lab Lighthouse)
  let health = 100;
  if (fcp != null) health -= Math.min(40, Math.max(0, (fcp - 1000) / 50));
  if (lcp != null) health -= Math.min(30, Math.max(0, (lcp - 1500) / 50));
  if (snap.cls != null) health -= Math.min(20, snap.cls * 100);
  health -= Math.min(20, longTaskTotal / 20);
  health -= Math.min(15, slowResources.length * 3);
  health = Math.max(0, Math.round(health));

  return { health, findings, slowResources: slowResources.slice(0, 10), longTaskTotal };
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');
  await cdp.send('Network.enable');

  // Install observers before navigation when we control the load.
  await cdp.send('Runtime.evaluate', {
    expression: `(() => {
      globalThis.__octocodeLongTasks = [];
      try {
        if ('PerformanceObserver' in globalThis) {
          const lt = new PerformanceObserver(list => {
            for (const e of list.getEntries()) {
              globalThis.__octocodeLongTasks.push({
                startTime: Math.round(e.startTime),
                duration: Math.round(e.duration),
                name: e.name,
              });
            }
          });
          lt.observe({ type: 'longtask', buffered: true });
          try {
            const lcpObs = new PerformanceObserver(() => {});
            lcpObs.observe({ type: 'largest-contentful-paint', buffered: true });
          } catch {}
          try {
            const clsObs = new PerformanceObserver(() => {});
            clsObs.observe({ type: 'layout-shift', buffered: true });
          } catch {}
        }
      } catch {}
    })()`,
  });

  const existing = process.env.MEASURE_EXISTING === '1';
  const measureUrl = process.env.MEASURE_URL || (existing ? null : FIXTURE);
  // Background tabs do not paint, so LCP never fires; measure the tab in front.
  await cdp.send('Page.bringToFront').catch(() => {});
  // Headless paints lazily: without frames, late content gets no LCP entry (or one stamped at the
  // next forced paint). A tiny screencast keeps frames coming so paint timings are real.
  const ackFrame = ({ sessionId }) => { cdp.send('Page.screencastFrameAck', { sessionId }).catch(() => {}); };
  cdp.on('Page.screencastFrame', ackFrame);
  await cdp.send('Page.startScreencast', { format: 'jpeg', quality: 5, maxWidth: 32, maxHeight: 32, everyNthFrame: 1 }).catch(() => {});
  if (measureUrl) {
    await cdp.send('Page.navigate', { url: measureUrl });
    await new Promise(r => setTimeout(r, Number(process.env.PERF_WAIT_MS || 1200)));
  } else {
    await new Promise(r => setTimeout(r, 300));
  }

  const raw = await collect(cdp);
  await cdp.send('Page.stopScreencast').catch(() => {});
  cdp.off?.('Page.screencastFrame', ackFrame);
  const snap = raw.result?.value || { resources: [], longTasks: [], paints: {} };
  const scored = score(snap, SLOW_RESOURCE_MS);
  const payload = { ...snap, score: scored, collectedAt: new Date().toISOString() };

  const artifact = join(cdp.outputDir, 'performance-measure.json');
  writeFileSync(artifact, `${JSON.stringify(payload, null, 2)}\n`, { mode: 0o600 });

  console.log(`[METRIC] PERF health=${scored.health} fcp=${snap.fcp ?? 'n/a'} lcp=${snap.lcp?.startTime ?? 'n/a'} cls=${snap.cls ?? 'n/a'} inp=${snap.inp ?? 'n/a'} longTasks=${snap.longTasks?.length ?? 0} resources=${snap.resources?.length ?? 0}`);
  if (snap.lcp?.element) console.log(`[METRIC] LCP element=${snap.lcp.element} size=${snap.lcp.size}${snap.lcp.url ? ` url=${snap.lcp.url}` : ''}`);
  if (snap.visibility && snap.visibility !== 'visible') console.log(`[FINDING] PERF_TAB_HIDDEN visibility=${snap.visibility}; browsers skip LCP for hidden tabs`);
  for (const f of scored.findings.slice(0, 8)) {
    console.log(`[FINDING] PERF_${f.code} ${JSON.stringify(f)}`);
  }
  if (snap.navigation) {
    console.log(`[METRIC] NAV dcl=${snap.navigation.domContentLoaded} load=${snap.navigation.load} ttfb≈${snap.navigation.responseStart}`);
  }
  console.log(`[ARTIFACT] PERFORMANCE ${artifact}`);
}
