/**
 * Coverage: used and unused JS bytes (Profiler precise coverage) and CSS bytes (rule usage) for a
 * page load (COVERAGE_URL) or an interaction window (COVERAGE_MS) on the current page.
 * Options: COVERAGE_URL, COVERAGE_MS, COVERAGE_CSS, COVERAGE_JS (booleans 1/0; both default on).
 * CSS counts a byte as used only inside a rule that applied. Writes coverage.json (per-script and per-sheet totals) and coverage-ranges.json (every unused
 * range); nothing is capped. Offsets are UTF-16 code units of the script or stylesheet text.
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Payload, Session } from '../cdp-connection.mjs';
import {
  countOption,
  flagOption,
  sleep,
  textOption,
  say,
} from './stream-options.mjs';

type Range = { start: number; end: number };
type Entry = {
  url: string;
  type: 'js' | 'css';
  id: string;
  totalBytes: number;
  usedBytes: number;
  unusedBytes: number;
  percentUsed: number;
};
type Saver = Session & {
  saveArtifact?: (name: string, data: unknown) => unknown;
};

type CoverageRange = { startOffset: number; endOffset: number; count: number };

/** Sorted, merged, non-overlapping ranges. */
export function mergeRanges(ranges: Range[]): Range[] {
  const sorted = ranges
    .filter(range => range.end > range.start)
    .toSorted((a, b) => a.start - b.start || a.end - b.end);
  const merged: Range[] = [];
  for (const range of sorted) {
    const last = merged.at(-1);
    if (last && range.start <= last.end)
      last.end = Math.max(last.end, range.end);
    else merged.push({ ...range });
  }
  return merged;
}

/**
 * The ranges of one script that ran. Block coverage nests ranges (a count-0 block inside a
 * called function): the innermost range covering an offset decides whether it ran.
 */
export function usedJsRanges(ranges: CoverageRange[]): Range[] {
  const points = ranges.flatMap(range => [
    { offset: range.startOffset, type: 0, range },
    { offset: range.endOffset, type: 1, range },
  ]);
  const length = (range: CoverageRange) => range.endOffset - range.startOffset;
  points.sort((a, b) => {
    if (a.offset !== b.offset) return a.offset - b.offset;
    if (a.type !== b.type) return b.type - a.type;
    return a.type === 0
      ? length(b.range) - length(a.range)
      : length(a.range) - length(b.range);
  });
  const counts: number[] = [];
  const used: Range[] = [];
  let last = 0;
  for (const point of points) {
    if (counts.length && last < point.offset && counts[counts.length - 1] > 0)
      used.push({ start: last, end: point.offset });
    last = point.offset;
    if (point.type === 0) counts.push(point.range.count);
    else counts.pop();
  }
  return mergeRanges(used);
}

/** The parts of [0, total) that no range in `used` covers. */
export function complement(used: Range[], total: number): Range[] {
  const gaps: Range[] = [];
  let at = 0;
  for (const range of mergeRanges(used)) {
    const start = Math.min(range.start, total);
    const end = Math.min(range.end, total);
    if (start > at) gaps.push({ start: at, end: start });
    at = Math.max(at, end);
  }
  if (at < total) gaps.push({ start: at, end: total });
  return gaps;
}

const sum = (ranges: Range[]) =>
  ranges.reduce((bytes, range) => bytes + (range.end - range.start), 0);

function entry(
  url: string,
  type: 'js' | 'css',
  id: string,
  total: number,
  unused: Range[]
): Entry {
  const unusedBytes = sum(unused);
  const usedBytes = total - unusedBytes;
  return {
    url,
    type,
    id,
    totalBytes: total,
    usedBytes,
    unusedBytes,
    percentUsed: total ? Number(((usedBytes / total) * 100).toFixed(2)) : 100,
  };
}

async function loaded(cdp: Session, url: string) {
  const done = new Promise<void>(resolve => {
    const onLoad = () => {
      cdp.off('Page.loadEventFired', onLoad);
      resolve();
    };
    cdp.on('Page.loadEventFired', onLoad);
  });
  await cdp.send('Page.navigate', { url });
  await Promise.race([done, sleep(30000)]);
}

export async function run(cdp: Session) {
  const wantJs = flagOption('COVERAGE_JS', true);
  const wantCss = flagOption('COVERAGE_CSS', true);
  if (!wantJs && !wantCss)
    throw new Error(
      'COVERAGE_JS and COVERAGE_CSS are both off: nothing to measure'
    );
  const url = textOption('COVERAGE_URL');
  const windowMs = countOption('COVERAGE_MS') ?? 0;

  const scriptLength = new Map<string, { url: string; length: number }>();
  const sheets = new Map<string, { url: string; length: number }>();
  cdp.on('Debugger.scriptParsed', (event: Payload) =>
    scriptLength.set(event.scriptId, {
      url: event.url || '',
      length: Number(event.length ?? 0),
    })
  );
  cdp.on('CSS.styleSheetAdded', ({ header }: Payload) =>
    sheets.set(header.styleSheetId, {
      url: header.sourceURL || '',
      length: Number(header.length ?? 0),
    })
  );

  await cdp.send('Page.enable');
  if (wantJs) {
    await cdp.send('Debugger.enable');
    await cdp.send('Profiler.enable');
    await cdp.send('Profiler.startPreciseCoverage', {
      callCount: true,
      detailed: true,
    });
  }
  if (wantCss) {
    await cdp.send('DOM.enable');
    await cdp.send('CSS.enable');
    await cdp.send('CSS.startRuleUsageTracking');
  }

  if (url) await loaded(cdp, url);
  if (windowMs) await sleep(windowMs);

  const entries: Entry[] = [];
  const unusedRanges: (Range & { url: string; type: string; id: string })[] =
    [];
  let skippedAnonymous = 0;

  if (wantJs) {
    const { result } = await cdp.send('Profiler.takePreciseCoverage');
    await cdp.send('Profiler.stopPreciseCoverage').catch(() => {});
    for (const script of result as Payload[]) {
      const info = scriptLength.get(script.scriptId);
      const scriptUrl: string = script.url || info?.url || '';
      if (!scriptUrl) {
        skippedAnonymous++;
        continue;
      }
      let total = info?.length ?? 0;
      if (!total) {
        const source = await cdp.send('Debugger.getScriptSource', {
          scriptId: script.scriptId,
        });
        total = String(source.scriptSource ?? '').length;
      }
      const used = usedJsRanges(
        (script.functions as Payload[]).flatMap(
          fn => fn.ranges as CoverageRange[]
        )
      );
      const unused = complement(used, total);
      entries.push(entry(scriptUrl, 'js', script.scriptId, total, unused));
      for (const range of unused)
        unusedRanges.push({
          url: scriptUrl,
          type: 'js',
          id: script.scriptId,
          ...range,
        });
    }
  }

  if (wantCss) {
    const { ruleUsage } = await cdp.send('CSS.stopRuleUsageTracking');
    // Chrome reports the rules that applied; every other byte of the sheet counts as unused
    // (the DevTools Coverage tab does the same), so unused = the sheet minus the used rules.
    const usedBySheet = new Map<string, Range[]>();
    for (const rule of ruleUsage as Payload[]) {
      if (!rule.used) continue;
      const list = usedBySheet.get(rule.styleSheetId) ?? [];
      list.push({ start: rule.startOffset, end: rule.endOffset });
      usedBySheet.set(rule.styleSheetId, list);
    }
    for (const [id, sheet] of sheets) {
      const sheetUrl = sheet.url || `inline-style:${id}`;
      const unused = complement(usedBySheet.get(id) ?? [], sheet.length);
      entries.push(entry(sheetUrl, 'css', id, sheet.length, unused));
      for (const range of unused)
        unusedRanges.push({ url: sheetUrl, type: 'css', id, ...range });
    }
  }

  const totals = Object.fromEntries(
    (['js', 'css'] as const).map(type => {
      const rows = entries.filter(row => row.type === type);
      const totalBytes = rows.reduce((n, row) => n + row.totalBytes, 0);
      const unusedBytes = rows.reduce((n, row) => n + row.unusedBytes, 0);
      return [
        type,
        {
          files: rows.length,
          totalBytes,
          usedBytes: totalBytes - unusedBytes,
          unusedBytes,
          percentUsed: totalBytes
            ? Number(
                (((totalBytes - unusedBytes) / totalBytes) * 100).toFixed(2)
              )
            : 100,
        },
      ];
    })
  );

  const report = {
    collectedAt: new Date().toISOString(),
    url: url ?? String(cdp.targetInfo.url ?? ''),
    windowMs,
    measured: { js: wantJs, css: wantCss },
    skippedAnonymousScripts: skippedAnonymous,
    totals,
    entries,
  };
  const saver = cdp as Saver;
  if (saver.saveArtifact) {
    saver.saveArtifact('coverage.json', report);
    saver.saveArtifact('coverage-ranges.json', { ranges: unusedRanges });
  } else {
    for (const [name, data] of [
      ['coverage.json', report],
      ['coverage-ranges.json', { ranges: unusedRanges }],
    ] as const) {
      const file = join(cdp.outputDir, name);
      writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`, {
        mode: 0o600,
      });
      say(`[ARTIFACT] ${name} ${file}`);
    }
  }

  for (const [type, row] of Object.entries(totals))
    if (measured(type, wantJs, wantCss))
      say(
        `[METRIC] COVERAGE ${type} files=${row.files} total=${row.totalBytes} used=${row.usedBytes} unused=${row.unusedBytes} percentUsed=${row.percentUsed}`
      );
  const top = entries
    .filter(row => row.unusedBytes > 0)
    .toSorted((a, b) => b.unusedBytes - a.unusedBytes)
    .slice(0, 20);
  for (const row of top)
    say(
      `[COVERAGE] ${row.type} unused=${row.unusedBytes} total=${row.totalBytes} percentUsed=${row.percentUsed} ${row.url}`
    );
  say(
    `[PAGE] shown=${top.length} of=${entries.filter(row => row.unusedBytes > 0).length} with unused bytes; every entry is in coverage.json and every unused range in coverage-ranges.json (read them with the artifact continuation)`
  );
  if (skippedAnonymous)
    say(
      `[FINDING] COVERAGE_ANONYMOUS_SCRIPTS skipped=${skippedAnonymous} scripts without a URL (evaluated code)`
    );
}

const measured = (type: string, js: boolean, css: boolean) =>
  type === 'js' ? js : css;
