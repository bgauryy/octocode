#!/usr/bin/env node
/**
 * Scan cost of a 256 KB web result and adversarial roughly 1 MB HTML, in CPU milliseconds (user + system of this
 * process). Three scans read a normal result before the model sees it:
 *
 *   provenance  `permission/provenance` indexes the text so a later call that repeats it is flagged (`record`);
 *               the cap is 256 KB per result, so this is the largest input it ever reads
 *   html-text   `web/html` turns 256 KB of HTML into text with the linear passes of `web/scan`
 *   check       `permission/provenance` looks one 500-character command up in the index (`check`)
 *
 * Reads the compiled package, so build first: `yarn workspace @octocodeai/pi-extension build`. Plain Node, no
 * dependencies. Numbers and the decision they led to: docs/CONTEXT-AND-COST.md#scan-cost-of-a-large-web-result.
 */
import { fileURLToPath } from 'node:url';
import { isMain } from './is-main.mjs';
import { adversarialPages } from './bench-web-scan-fixtures.ts';

const HELP = `Usage: bench-web-scan.mjs [options]

Prints CPU time (ms) for 256 KB web scans and adversarial roughly 1 MB HTML: best, median and worst of the runs,
each on a fresh index or the same immutable string. The first run of each case is a warm-up, shown apart as "first" (the first case's is the
process's first scan, before the code is compiled by the JIT) and not counted in the rest.

Options:
  --runs <n>   Counted runs per case (default 15)
  --json       Print JSON
  --check      Fail if any unique provenance sample takes >=200 CPU ms, or any adversarial HTML sample takes >=2000 CPU ms
  --help       Show this help
`;

const BYTES = 256 * 1024;
const ROOT = fileURLToPath(new URL('..', import.meta.url));
const CTX = { cwd: ROOT };

/** Text with no repeated 32-character window: every shingle is new to the index (the slowest input). */
function uniqueText(length) {
  let out = '';
  for (let n = 0; out.length < length; n++) out += `perf${n.toString(36)} `;
  return out.slice(0, length);
}

/** Prose-like text: a small vocabulary in a fixed pseudo-random order, so windows repeat as they do on real pages. */
function proseText(length) {
  const words = ['the', 'install', 'step', 'run', 'configure', 'server', 'then', 'open', 'file', 'and', 'check', 'logs', 'for', 'errors', 'before', 'you', 'deploy', 'a', 'new', 'build'];
  let out = '';
  let seed = 7;
  while (out.length < length) {
    seed = (Math.imul(seed, 1103515245) + 12345) >>> 0;
    out += `${words[seed % words.length]} `;
  }
  return out.slice(0, length);
}

/** A page of paragraphs, links, lists, scripts and hidden blocks, about `length` characters. */
function htmlPage(length) {
  const row = (n) => `<div class="row"><h2>Section ${n}</h2><p>Text of section ${n} with <a href="https://example.com/${n}">a link</a> &amp; more.</p><ul><li>one</li><li>two</li></ul><script>var x${n}=${n};</script><span hidden>hidden ${n}</span></div>\n`;
  let out = '<html><head><title>t</title></head><body><nav>menu</nav>';
  for (let n = 0; out.length < length; n++) out += row(n);
  return `${out}</body></html>`;
}

const cpuMs = (since) => {
  const used = process.cpuUsage(since);
  return (used.user + used.system) / 1000;
};

function measure(runs, prepare, work) {
  const times = [];
  let first = 0;
  for (let run = 0; run <= runs; run++) {
    const input = prepare();
    const started = process.cpuUsage();
    work(input);
    if (run > 0) times.push(cpuMs(started));
    else first = cpuMs(started);
  }
  times.sort((a, b) => a - b);
  return { first, best: times[0], median: times[Math.floor(times.length / 2)], worst: times.at(-1), runs };
}

/** The scans, each as `[name, run, optional CPU budget]`. */
export async function scans(runs) {
  const { createProvenance } = await import('../dist/permission/provenance.js');
  const { htmlToText, describeHtml } = await import('../dist/web/html.js');
  const newIndex = () => createProvenance({ workspaceRoot: () => ROOT, now: () => 0 });
  const result = (text) => ({ type: 'tool_result', toolCallId: 'bench', toolName: 'web', input: { url: 'https://example.com/big' }, content: [{ type: 'text', text }], isError: false, details: undefined });
  const unique = uniqueText(BYTES);
  const prose = proseText(BYTES);
  const page = htmlPage(BYTES);
  const command = { type: 'tool_call', toolCallId: 'bench', toolName: 'bash', input: { command: unique.slice(-500) } };
  return [
    ['provenance record, unique text', () => measure(runs, newIndex, (index) => index.record(result(unique), CTX)), 200],
    ['provenance record, prose', () => measure(runs, newIndex, (index) => index.record(result(prose), CTX))],
    ['html to text, 256 KB page', () => measure(runs, () => page, (html) => htmlToText(html))],
    ['provenance check, 500 characters', () => {
      const indexed = newIndex();
      indexed.record(result(unique), CTX);
      return measure(runs, () => indexed, (index) => index.check(command, CTX));
    }],
    ...adversarialPages().map(([name, html]) => [`adversarial HTML, ${name}`, () => measure(runs, () => html, (input) => describeHtml(input, 'https://example.com/')), 2000]),
  ];
}

export function assertBudgets(rows) {
  for (const row of rows) {
    if (row.budgetMs !== undefined && ![row.first, row.worst].every(value => Number.isFinite(value) && value < row.budgetMs)) {
      throw new Error(`${row.name}: CPU samples must be <${row.budgetMs} ms (first ${row.first}, worst ${row.worst})`);
    }
  }
}

async function main(argv) {
  if (argv.includes('--help')) return process.stdout.write(HELP);
  const at = argv.indexOf('--runs');
  const runs = at < 0 ? 15 : Number(argv[at + 1]);
  if (!Number.isInteger(runs) || runs < 1) throw new Error('--runs takes a positive integer');
  const rows = [];
  for (const [name, run, budgetMs] of await scans(runs)) rows.push({ name, ...run(), ...(budgetMs !== undefined && { budgetMs }) });
  if (argv.includes('--check')) assertBudgets(rows);
  if (argv.includes('--json')) return process.stdout.write(`${JSON.stringify({ bytes: BYTES, rows }, null, 2)}\n`);
  const fixed = (value) => value.toFixed(1).padStart(7);
  process.stdout.write(`CPU ms per scan: ${BYTES / 1024} KB normal inputs; roughly 1 MB adversarial HTML (${runs} runs after a warm-up)\n${'scan'.padEnd(36)}${'first'.padStart(7)}${'best'.padStart(8)}${'median'.padStart(8)}${'worst'.padStart(8)}\n`);
  for (const row of rows) process.stdout.write(`${row.name.padEnd(36)}${fixed(row.first)} ${fixed(row.best)} ${fixed(row.median)} ${fixed(row.worst)}\n`);
}

if (isMain(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
}
