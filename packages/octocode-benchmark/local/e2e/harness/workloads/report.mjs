// Markdown view of a workloads run: one table per group, then the failures.
const mb = kb => (kb ? (kb / 1024).toFixed(0) : '–');
const kib = bytes => (bytes == null ? '–' : bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KiB`);
const ms = x => (x == null ? '–' : String(x));
const pct = (xs, p) => { const s = xs.filter(x => x != null).toSorted((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : null; };

function summaryRow(g, rs) {
  const peaks = rs.map(r => r.peakFootprintKb);
  return `| ${g} | ${rs.length} | ${rs.filter(r => r.ok).length} | ${ms(pct(rs.map(r => r.coldMs), 0.5))} | ${ms(pct(rs.map(r => r.coldMs), 0.95))} | ${ms(pct(rs.map(r => r.warmP50Ms), 0.5))} | ${mb(pct(peaks, 0.5))} | ${mb(Math.max(0, ...peaks.filter(Boolean)))} | ${mb(Math.max(0, ...rs.map(r => r.sampledTreeKb ?? 0)))} |`;
}

const caseRow = r => `| ${r.name} | ${r.ok ? '✓' : r.unavailable ? 'n/a' : '✗'} | ${ms(r.coldMs)} | ${ms(r.warmP50Ms)} | ${kib(r.bytes)} | ${mb(r.peakFootprintKb)} | ${mb(r.sampledRssKb ?? r.peakRssKb)} | ${mb(r.sampledTreeKb)} |`;
const stepLine = s => `${s.tool} ${s.missing ? `(${s.missing})` : `${s.ms} ms${s.error ? ` ✗ ${s.error}` : ''}`}`;

export function renderMarkdown(run) {
  const { host, binary, rows } = run;
  const groups = [...new Set(rows.map(r => r.group))];
  const lines = [
    '# Workload latency and peak memory', '',
    `${run.at} · ${host.platform}-${host.arch} · ${host.cpuModel} · ${host.cpus} CPUs · ${host.memGb} GB · load at end ${host.loadAtEnd.map(x => x.toFixed(1)).join(' ')}`,
    `Binary \`${binary?.file}\` (${binary ? (binary.bytes / 2 ** 20).toFixed(0) : '?'} MB, sha256 ${binary?.sha256?.slice(0, 12)}). Each case: fresh \`octocode mcp\`, 1 cold call + ${run.repeat} warm. Footprint is the server's exact peak physical footprint (macOS \`time -l\`); server RSS is sampled every 100 ms; tree adds language servers and other children (sampled). rusage max RSS counts reaped children too and is kept only in the JSON.`, '',
    '## Summary', '',
    '| group | cases | ok | cold p50 ms | cold p95 ms | warm p50 ms | footprint p50 MB | footprint max MB | tree max MB |',
    '|---|---:|---:|---:|---:|---:|---:|---:|---:|',
  ];
  for (const g of groups) lines.push(summaryRow(g, rows.filter(r => r.group === g)));
  for (const g of groups) {
    lines.push('', `## ${g}`, '', '| case | ok | cold ms | warm p50 ms | response | footprint MB | server RSS MB | tree MB |', '|---|:-:|---:|---:|---:|---:|---:|---:|');
    lines.push(...rows.filter(x => x.group === g).map(caseRow));
    for (const r of rows.filter(x => x.group === g && x.steps?.length > 1)) lines.push('', `- ${r.name}: ${r.steps.map(stepLine).join(' → ')}`);
  }
  const failed = rows.filter(r => !r.ok && !r.unavailable);
  const unavailable = rows.filter(r => r.unavailable).map(r => r.name);
  if (unavailable.length) lines.push('', `No language server on this machine (n/a): ${unavailable.join(', ')}.`);
  if (failed.length) {
    lines.push('', '## Failures', '');
    for (const r of failed) lines.push(`- **${r.group} ${r.name}**: ${String(r.error ?? 'unknown').replace(/\s+/g, ' ').slice(0, 300)}`);
  }
  return `${lines.join('\n')}\n`;
}
