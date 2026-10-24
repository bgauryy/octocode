// The soak output directory: calls.jsonl, stderr.log, samples/ and the summary.{json,md} report.
import fs from 'node:fs';
import path from 'node:path';

/** The run's output files under `dir`; `prepare()` clears a previous run's files. */
export function createOutput(dir) {
  const samples = path.join(dir, 'samples');
  return {
    dir, samples,
    prepare() {
      fs.mkdirSync(dir, { recursive: true });
      for (const f of ['summary.json', 'summary.md', 'calls.jsonl', 'stderr.log']) fs.rmSync(path.join(dir, f), { force: true });
      fs.rmSync(samples, { recursive: true, force: true });
      fs.mkdirSync(samples, { recursive: true });
    },
    tee: text => fs.appendFileSync(path.join(dir, 'stderr.log'), text),
    record: row => fs.appendFileSync(path.join(dir, 'calls.jsonl'), `${JSON.stringify(row)}\n`),
  };
}

const toolsSection = tools => [
  '## Tools', '', '| tool | calls | errors | breaches | p50 ms | p95 ms | max ms | avg bytes |', '|---|---|---|---|---|---|---|---|',
  ...tools.map(s => `| ${s.tool} | ${s.calls} | ${s.errors} | ${s.breaches} | ${s.p50} | ${s.p95} | ${s.max} | ${s.avgBytes} |`), '',
];

function windowLine(w, rss) {
  const r = rss.find(x => x.window === w.window);
  const growth = r?.ok === null ? 'n/a' : `${Math.round(r.growth * 100)}%`;
  return `| ${w.window} | ${w.pid} | ${w.reason} | ${w.startMs} | ${w.endReason ?? ''} | ${w.firstRssKb ?? '?'}→${w.lastRssKb ?? '?'} | ${w.maxTreeKb} | ${growth} |`;
}

const windowsSection = ({ windows, rss }) => (windows ? [
  '## Server windows', '', '| # | pid | start reason | start ms | end | RSS first→last KB | max tree KB | growth |', '|---|---|---|---|---|---|---|---|',
  ...windows.map(w => windowLine(w, rss)), '',
] : []);

const driftLine = d => `- ${d.tool}: ${d.ok === null ? `n/a (${d.note})` : `${d.firstP95}→${d.lastP95} ms = ${d.ratio}x ${d.ok ? 'ok' : 'FAIL'}`}`;

/** summary.json and summary.md for a finished run. */
export function writeReport(out, summary) {
  fs.writeFileSync(path.join(out.dir, 'summary.json'), JSON.stringify(summary, null, 2));
  const md = [
    `# Soak ${summary.mode} — ${summary.verdict}`, '',
    `${summary.at} · ${summary.runSeconds}s · ${summary.calls} calls`, '',
    ...(summary.reasons.length ? ['## Reasons', '', ...summary.reasons.map(r => `- ${r}`), ''] : []),
    ...toolsSection(summary.tools),
    ...windowsSection(summary),
    '## Drift (p95 first vs last span)', '', ...summary.drift.map(driftLine), '',
    ...(summary.events.length ? ['## Events', '', ...summary.events.map(e => `- \`${JSON.stringify(e)}\``), ''] : []),
  ];
  fs.writeFileSync(path.join(out.dir, 'summary.md'), md.join('\n'));
}
