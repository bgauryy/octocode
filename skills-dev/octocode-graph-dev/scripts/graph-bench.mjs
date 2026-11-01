#!/usr/bin/env node
// Graph language battery: per pinned grammar repository, the agent workflow
// graphBuild → graphQuery stats → find → callers → impact → issues → cycles → unresolved, plus
// a file-level recall/precision check of `callers` against ripgrep call sites (lib/oracle.mjs).
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { excludedBy, fileRecall, missedSites, oracleSites } from './lib/oracle.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

// One probe function per grammar; `dir` scopes the ingest, `ext` the ripgrep oracle, `exclude`
// (globs relative to `dir`) drops vendored copies and amalgamations from both sides of recall.
export const CASES = [
  { lang: 'asm+c', repo: 'asm', dir: '.', probe: 'jpeg_std_error', ext: 'c' },
  { lang: 'c', repo: 'c', dir: 'src', probe: 'createStringObject', ext: 'c' },
  { lang: 'cpp', repo: 'cpp', dir: '.', probe: 'skip_whitespace', ext: 'hpp', exclude: ['single_include/**'] },
  { lang: 'csharp', repo: 'csharp', dir: 'Src/Newtonsoft.Json', probe: 'ArgumentNotNull', ext: 'cs' },
  { lang: 'go', repo: 'go', dir: '.', probe: 'FromStrings', ext: 'go' },
  { lang: 'java', repo: 'java', dir: 'guava/src', probe: 'checkNotNull', ext: 'java' },
  { lang: 'javascript', repo: 'javascript', dir: '.', probe: 'isObject', ext: 'js', exclude: ['vendor/**'] },
  { lang: 'python', repo: 'python', dir: 'django', probe: 'force_str', ext: 'py' },
  { lang: 'rust', repo: 'rust', dir: 'tokio', probe: 'poll_proceed', ext: 'rs' },
  { lang: 'scala', repo: 'scala', dir: 'core', probe: 'tailRecM', ext: 'scala' },
  { lang: 'typescript', repo: 'typescript', dir: 'packages', probe: 'areTestsFiltered', ext: 'ts' },
  { lang: 'cuda', repo: 'cuda', dir: 'tools/library', probe: 'make_gemm_real_canonical_layouts', ext: 'cu,cpp,h,hpp,cuh' },
  { lang: 'tsx', repo: 'tsx', dir: 'packages', probe: 'mutateElement', ext: 'ts,tsx' },
];

const HELP = `Usage: graph-bench.mjs [--out FILE] [--only repo,...] [--bin PATH] [--repos DIR] [--jobs N] [--edges]

  --out    result JSON (default <repo>/.octocode/octocode-graph-dev/<date>-battery/bench.json, fresh)
  --only   comma-separated repo directories from the case list (default: every case; a missing clone becomes an ingest-error row)
  --bin    native octocode binary or launcher .js (default: OCTO_BIN, else the repo launcher)
  --repos  pinned clones (default packages/octocode-benchmark/local/repos)
  --jobs   repositories benchmarked at once (default 4; use 1 for timing)
  --edges  also list every call edge into the probe (witnesses: from, to, line, confidence)
Compare two runs with bench-compare.mjs.`;

function parseArgs(argv) {
  const opts = { only: [], jobs: 4 };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--help' || flag === '-h') opts.help = true;
    else if (flag === '--edges') opts.edges = true;
    else if (['--out', '--only', '--bin', '--repos', '--jobs'].includes(flag)) {
      const value = argv[++i];
      if (value === undefined) throw new Error(`${flag} needs a value`);
      if (flag === '--only') opts.only = value.split(',').filter(Boolean);
      else if (flag === '--jobs') opts.jobs = Math.max(1, Number(value) || 1);
      else opts[flag.slice(2)] = value;
    } else throw new Error(`unknown argument ${flag}\n\n${HELP}`);
  }
  return opts;
}

function freshOut() {
  const base = join(REPO, '.octocode/octocode-graph-dev', `${new Date().toISOString().slice(0, 10)}-battery`);
  let dir = base;
  for (let n = 2; existsSync(dir); n++) dir = `${base}-${n}`;
  return join(dir, 'bench.json');
}

function run(cmd, args, cwd) {
  return new Promise((done) => {
    const started = Date.now();
    const child = spawn(cmd, args, { cwd });
    let out = '';
    let err = '';
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', (e) => done({ code: -1, out, err: String(e), ms: Date.now() - started }));
    child.on('close', (code) => done({ code, out, err, ms: Date.now() - started }));
  });
}

// A tool response's row data (graphBuild/graphQuery print `{"results":[{"data":…}]}`); a parse failure is a row.
const parse = (r) => {
  try {
    const response = JSON.parse(r.out);
    return { ...(response.results?.[0]?.data ?? {}), responsePagination: response.responsePagination };
  } catch {
    return { parseError: (r.out || r.err).slice(0, 300) };
  }
};
// The approved contract maximum: a page holds as many rows as fit.
const RESPONSE_CHARS = 50000;
const fileOf = (id) => String(id ?? '').split('#')[0];
const leafName = (id) => id?.split('#')[1]?.split('.').pop()?.replace(/@\d+$/, '');

// One CLI client per run: the generated tool commands (`octocode graphQuery --json '<envelope>'`), a query helper
// pinned to one snapshot and scope, and a pager that follows every continuation the tool returns.
function client(opts, cwd) {
  const octo = (tool, input) => {
    const args = [tool, '--json', JSON.stringify(input)];
    return opts.bin.endsWith('.js') ? run('node', [opts.bin, ...args], cwd) : run(opts.bin, args, cwd);
  };
  const call = async (tool, input) => {
    const res = await octo(tool, input);
    return { res, j: parse(res) };
  };
  const withGraph = (path, id) => {
    const q = (fields) => call('graphQuery', { queries: [{ ...fields, path, graph: id }], responseChars: RESPONSE_CHARS });
    // Every page of a list: the serialized-response page first, then the row's next.nextPage, until neither remains.
    const pages = async (fields) => {
      const rows = [];
      let ms = 0, bytes = 0, calls = 0, page = await q(fields), first = page.j;
      for (;;) {
        calls++;
        ms += page.res.ms;
        bytes += page.res.out.length;
        rows.push(...(page.j.rows ?? []));
        const next = page.j.responsePagination?.next?.input ?? page.j.next?.nextPage?.input;
        if (!next || page.j.error) break;
        page = await call('graphQuery', next);
      }
      return { rows, ms, bytes, calls, total: first.pagination?.totalItems ?? rows.length, pagesComplete: !page.j.error && rows.length === (first.pagination?.totalItems ?? rows.length) };
    };
    return { q, pages };
  };
  return { call, withGraph };
}

function ingestRow(ing, r) {
  return {
    ms: ing.ms,
    exit: ing.code,
    error: r.error ?? r.parseError,
    files: r.counts?.nodesByKind?.file,
    symbols: r.counts?.nodesByKind?.symbol,
    edges: r.counts?.edges,
    callSites: r.calls?.sites,
    callLinkRate: r.calls?.sites ? +(r.calls.linked / r.calls.sites).toFixed(3) : null,
    unresolvedByReason: r.calls?.unresolvedByReason,
    diagnostics: r.diagnostics,
    scan: r.scan,
    linkage: r.linkage,
  };
}

// Callers of every definition named `probe`, as files.
async function callerFiles(pages, ids) {
  const files = new Set();
  const row = { definitions: ids.length, rows: 0, files: 0, ms: 0, bytes: 0, pagesComplete: true };
  for (const id of ids) {
    const cl = await pages({ operation: 'callers', target: id, confidence: 'low', limit: 1000 });
    row.rows += cl.rows.length;
    row.ms += cl.ms;
    row.bytes += cl.bytes;
    row.pagesComplete &&= cl.pagesComplete;
    for (const x of cl.rows) files.add(fileOf(x.id ?? x.from ?? x.src));
  }
  row.files = files.size;
  return { files, row };
}

// Oracle: call-shaped text hits (`name(`, `name<`, `name[`; never `rg -w` with a trailing `(`),
// minus the probe's own declarations, excluded files, strings and comments. Each missed file's
// sites carry the graph's unresolved row for the probe, so a miss names its reason.
async function oracle(c, root, graphFiles, definitions, pages) {
  const globs = c.ext.split(',').flatMap((e) => ['-g', `*.${e}`]);
  const rg = await run('rg', ['--json', '--no-ignore-vcs', ...globs, `\\b${c.probe}\\s*[(<\\[]`, '.'], root);
  const matches = [];
  for (const line of rg.out.split('\n')) {
    const event = line && JSON.parse(line);
    if (event?.type !== 'match' || event.data.path.text === undefined) continue;
    const file = event.data.path.text.replace(/^\.\//, '');
    for (const s of event.data.submatches) matches.push({ file, line: event.data.line_number, offset: event.data.absolute_offset + s.start });
  }
  const exclude = c.exclude ?? [];
  const text = (file) => readFileSync(join(root, file));
  const { sites, dropped } = oracleSites({ matches, definitions, exclude, text });
  const graphKept = new Set([...graphFiles].filter((f) => !excludedBy(exclude)(f)));
  const row = { oracleExit: rg.code, exclude, dropped, ...fileRecall(new Set(sites.map((s) => s.file)), graphKept) };
  const unresolved = [];
  for (let i = 0; i < row.missed.length; i += 100) {
    const page = await pages({ operation: 'unresolved', target: row.missed.slice(i, i + 100), edges: ['calls'], limit: 1000 });
    unresolved.push(...page.rows);
  }
  row.missedSites = missedSites({ missed: row.missed, sites, unresolved, probe: c.probe });
  return row;
}

// Every call edge into the probe's definitions, for --edges.
async function probeEdges(pages, ids) {
  const edges = [];
  for (const id of ids) {
    const w = await pages({ operation: 'witnesses', to: id, edges: ['calls'], confidence: 'low', limit: 1000 });
    for (const x of w.rows) edges.push({ from: x.from, to: x.to, line: x.line, confidence: x.confidence, detail: x.detail });
  }
  return edges;
}

// The rest of the agent workflow on the snapshot: impact, issues, cycles, unresolved.
async function workflow(q, firstId) {
  const out = {};
  if (firstId) {
    const im = await q({ operation: 'impact', target: firstId, limit: 20 });
    out.impact = { ms: im.res.ms, exit: im.res.code, total: im.j.pagination?.totalItems ?? im.j.rows?.length, bytes: im.res.out.length, summaryKeys: Object.keys(im.j.summary ?? {}) };
  }
  const is = await q({ operation: 'issues', limit: 200 });
  const by = {};
  for (const f of is.j.rows ?? []) by[`${f.detector}/t${f.tier}`] = (by[`${f.detector}/t${f.tier}`] ?? 0) + 1;
  out.issues = { ms: is.res.ms, exit: is.res.code, total: is.j.pagination?.totalItems ?? is.j.rows?.length, shown: (is.j.rows ?? []).length, hidden: is.j.summary?.hiddenBelowTier, entrypoints: is.j.summary?.entrypoints, notes: is.j.summary?.notes, by, error: is.j.error };
  const cy = await q({ operation: 'cycles', limit: 5 });
  out.cycles = { ms: cy.res.ms, exit: cy.res.code, total: cy.j.pagination?.totalItems ?? cy.j.rows?.length, error: cy.j.error };
  const un = await q({ operation: 'unresolved', limit: 5 });
  out.unresolved = { exit: un.res.code, total: un.j.pagination?.totalItems ?? un.j.rows?.length, sample: (un.j.rows ?? []).slice(0, 5) };
  return out;
}

async function bench(c, opts) {
  const cwd = join(opts.repos, c.repo);
  const root = join(cwd, c.dir);
  const row = { lang: c.lang, repo: c.repo, dir: c.dir, probe: c.probe };
  if (!existsSync(root)) return { ...row, ingest: { error: `missing clone ${root}` } };
  const { call, withGraph } = client(opts, cwd);
  const { res: ing, j: r } = await call('graphBuild', { queries: [{ path: root, refresh: true }] });
  row.ingest = ingestRow(ing, r);
  if (row.ingest.error || !r.id) return row;
  const { q, pages } = withGraph(root, r.id);
  const stats = await q({ operation: 'stats' });
  row.stats = { ms: stats.res.ms, bytes: stats.res.out.length, exit: stats.res.code, keys: Object.keys(stats.j) };
  row.statsBody = stats.j;
  const find = await pages({ operation: 'find', target: c.probe, kind: 'symbol', limit: 1000, includeProvenance: true });
  row.find = { ms: find.ms, total: find.total, first: find.rows.slice(0, 5).map((x) => x.id ?? x) };
  const defs = find.rows.filter((x) => leafName(x.id) === c.probe);
  const ids = defs.map((x) => x.id);
  const callers = await callerFiles(pages, ids);
  row.callers = callers.row;
  const definitions = defs.filter((x) => x.selectionByteRange).map((x) => ({ file: x.file, startByte: x.selectionByteRange.startByte }));
  row.recall = await oracle(c, root, callers.files, definitions, pages);
  if (opts.edges) row.edges = await probeEdges(pages, ids);
  return { ...row, ...(await workflow(q, ids[0])) };
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) return console.log(HELP);
  opts.bin = resolve(opts.bin ?? process.env.OCTO_BIN ?? join(REPO, 'packages/octocode/out/octocode.js'));
  opts.repos = resolve(opts.repos ?? join(REPO, 'packages/octocode-benchmark/local/repos'));
  opts.out = resolve(opts.out ?? freshOut());
  const unknown = opts.only.filter((repo) => !CASES.some((c) => c.repo === repo));
  if (unknown.length) throw new Error(`unknown --only repo: ${unknown.join(', ')}`);
  mkdirSync(dirname(opts.out), { recursive: true });
  const queue = CASES.filter((c) => !opts.only.length || opts.only.includes(c.repo));
  const results = [];
  await Promise.all(
    Array.from({ length: opts.jobs }, async () => {
      while (queue.length) {
        const row = await bench(queue.shift(), opts);
        results.push(row);
        console.log(`${row.lang}: ingest ${row.ingest.ms ?? '-'}ms ${row.ingest.error ?? ''} link=${row.ingest.callLinkRate} recall=${row.recall?.recall} precision=${row.recall?.precision}`);
        results.sort((a, b) => a.lang.localeCompare(b.lang));
        writeFileSync(opts.out, `${JSON.stringify(results, null, 1)}\n`);
      }
    }),
  );
  console.log(`saved ${opts.out}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e.message);
    process.exit(2);
  });
}
