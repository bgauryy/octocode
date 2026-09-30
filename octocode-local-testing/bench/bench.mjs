#!/usr/bin/env node
// Repeatable benchmark: octocode (with/without clasify) vs gh and rg/find/sed.
//   node octocode-local-testing/bench/bench.mjs --suite pr|local|all --label <name> [--only id,id] [--arms a,b]
//   node octocode-local-testing/bench/bench.mjs --freeze-gt     # re-freeze symbol-usage ground truth
// Writes bench/results/bench-<label>.json and bench-<label>.md. See BENCHMARK.md.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { startServer, nextHints, collect } from '../harness/mcp-client.mjs';
import { PR_TASKS, LOCAL_TASKS } from './tasks.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPOS = path.resolve(HERE, '../repos');
const RESULTS = path.join(HERE, 'results');
const GT_FILE = path.join(HERE, 'ground-truth.json');

const argv = process.argv.slice(2);
const arg = (name, def) => { const i = argv.indexOf(`--${name}`); return i >= 0 ? argv[i + 1] : def; };
const SUITE = arg('suite', 'all');
const LABEL = arg('label', 'run');
const ONLY = arg('only')?.split(',');
const ARMS = arg('arms')?.split(',');
const K_READ = 3; // files an rg/octocode agent opens after a discovery search
const CLASIFY_READ_MIN = 0.5; // clasify guidance: read at exists/P >= 0.5

// ---------------------------------------------------------------- shell
function resolveRg() {
  if (process.env.RG_BIN) return process.env.RG_BIN;
  const found = spawnSync('/bin/sh', ['-c', 'command -v rg'], { encoding: 'utf8' }).stdout.trim();
  if (found.startsWith('/')) return found;
  const cc = process.env.CLAUDE_CODE_EXECPATH || path.join(os.homedir(), '.local/bin/claude');
  if (fs.existsSync(cc)) return `ARGV0=rg ${cc}`; // Claude Code bundles ripgrep
  throw new Error('ripgrep not found; set RG_BIN');
}
const RG = resolveRg();
const q = (s) => `'${String(s).replaceAll("'", "'\\''")}'`;
function sh(cmd, cwd) {
  const t = Date.now();
  const r = spawnSync('/bin/zsh', ['-c', cmd], { cwd, encoding: 'utf8', maxBuffer: 1 << 30 });
  // The agent sees stdout and stderr.
  return { cmd, out: (r.stdout ?? '') + (r.stderr ?? ''), stdout: r.stdout ?? '', ms: Date.now() - t, code: r.status };
}

// ---------------------------------------------------------------- recording
function recorder() {
  const steps = [];
  const opened = new Map(); // path -> { text, read }
  return {
    steps, opened,
    step(label, via, chars, ms, error = false) { steps.push({ label, via, chars, ms, error }); },
    shell(label, cmd, cwd) { const r = sh(cmd, cwd); steps.push({ label, via: 'shell', cmd: cmd.length > 300 ? cmd.slice(0, 300) + '…' : cmd, chars: r.out.length, ms: r.ms, error: r.code !== 0 && !r.stdout }); return r; },
    add(p, text, read) { const e = opened.get(p) ?? { text: '', read: false }; e.text += `\n${text}`; e.read ||= !!read && text.trim().length > 0; opened.set(p, e); },
  };
}
async function octo(rec, c, label, tool, args, { raw = false } = {}) {
  const e = raw ? await c.raw(tool, args) : await c.call(tool, args);
  rec.steps.push({ label, via: tool, chars: e.text.length, ms: e.ms, error: e.isError || e.rowErrors > 0 });
  return e;
}
function summarize(rec, judge) {
  const chars = rec.steps.reduce((a, s) => a + s.chars, 0);
  return { chars, tokens: Math.round(chars / 4), calls: rec.steps.length, ms: rec.steps.reduce((a, s) => a + s.ms, 0), errors: rec.steps.filter((s) => s.error).length, ...judge, steps: rec.steps };
}

// Judge: a GT file counts as found when every marker appears in text the arm saw for that file.
function judge(rec, gt, { usages } = {}) {
  const found = {};
  for (const [file, markers] of Object.entries(gt)) {
    const text = rec.opened.get(file)?.text ?? '';
    found[file] = markers.every((m) => text.includes(m));
  }
  const gtFiles = Object.keys(gt);
  const hit = gtFiles.filter((f) => found[f]).length;
  const read = [...rec.opened].filter(([, v]) => v.read).map(([k]) => k);
  const readGt = read.filter((f) => gt[f]).length;
  const out = { recall: +(hit / gtFiles.length).toFixed(2), filesRead: read.length, readPrecision: read.length ? +(readGt / read.length).toFixed(3) : 0, found };
  if (usages) {
    const listed = rec.listed ?? new Set();
    const tp = usages.filter((u) => listed.has(u)).length;
    out.usageRecall = +(tp / usages.length).toFixed(2);
    out.usagePrecision = listed.size ? +(tp / listed.size).toFixed(2) : 0;
    out.correct = hit === gtFiles.length && out.usageRecall === 1 && out.usagePrecision === 1;
  } else out.correct = hit === gtFiles.length;
  return out;
}

// ================================================================= PR suite
const ghJson = (args) => JSON.parse(spawnSync('gh', args, { encoding: 'utf8', maxBuffer: 1 << 30 }).stdout);
const prFiles = (t) => ghJson(['api', '--paginate', '--slurp', `repos/${t.owner}/${t.repo}/pulls/${t.number}/files?per_page=100`]).flat();

function verifyPrGt(t, api) {
  const problems = [];
  for (const [file, markers] of Object.entries(t.gt)) {
    const f = api.find((x) => x.filename === file);
    if (!f) { problems.push(`${file} not in PR`); continue; }
    if (t.metaOnly?.includes(file)) { if (!markers.every((m) => f.previous_filename === m)) problems.push(`${file} previous path`); continue; }
    if (f.patch == null) continue; // patchless: verified via followup read below
    if (!markers.every((m) => f.patch.includes(m))) problems.push(`${file} markers missing from patch`);
  }
  return problems;
}

function parseUnifiedDiff(rec, text) {
  const parts = text.split(/^diff --git /m).slice(1);
  for (const part of parts) {
    const m = /^a\/(\S+) b\/(\S+)/.exec(part);
    if (m) rec.add(m[2], part, /^[-+][^-+]/m.test(part));
  }
}

function prGhTypical(t, api, sha) {
  const rec = recorder();
  const r = `${t.owner}/${t.repo}`;
  rec.shell('pr view', `gh pr view ${t.number} -R ${r}`);
  const diff = rec.shell('pr diff', `gh pr diff ${t.number} -R ${r}`);
  if (diff.code === 0 && diff.stdout.startsWith('diff --git')) parseUnifiedDiff(rec, diff.stdout);
  else {
    rec.shell('files api (diff too large)', `gh api --paginate 'repos/${r}/pulls/${t.number}/files?per_page=100'`);
    for (const f of api) rec.add(f.filename, `${f.status} ${f.previous_filename ?? ''}\n${f.patch ?? ''}`, f.patch != null);
  }
  for (const f of api) {
    if (!t.followup?.filter(f.filename) || f.patch != null || rec.opened.get(f.filename)?.read) continue;
    const raw = rec.shell(`raw ${f.filename}`, `gh api 'repos/${r}/contents/${f.filename}?ref=${sha}' -H 'Accept: application/vnd.github.raw'`);
    rec.add(f.filename, raw.stdout, true);
  }
  return summarize(rec, judge(rec, t.gt));
}

function prGhLean(t, api, sha) {
  const rec = recorder();
  const r = `${t.owner}/${t.repo}`;
  rec.shell('pr view --json', `gh pr view ${t.number} -R ${r} --json title,state,headRefOid,changedFiles,additions,deletions`);
  const sel = t.leanSelect ?? `((.patch // "") | test(${JSON.stringify(t.leanFileRe)}))`;
  const jq = `.[] | select((${sel}) or (.patch == null and .changes > 0)) | "### \\(.filename)\\t\\(.status)\\t\\(.previous_filename // "")\\t\\(if .patch == null then "NO_PATCH" else "" end)\\n" + ((.patch // "") | split("\\n") | map(select(test(${JSON.stringify(t.leanLineRe)}))) | join("\\n"))`;
  const out = rec.shell('files --jq filter', `gh api --paginate 'repos/${r}/pulls/${t.number}/files?per_page=100' --jq ${q(jq)}`);
  const noPatch = [];
  for (const block of out.stdout.split(/^### /m).slice(1)) {
    const [head, ...body] = block.split('\n');
    const [file, status, prev, flag] = head.split('\t');
    rec.add(file, `${status} ${prev}`, false);
    if (body.join('').trim()) rec.add(file, body.join('\n'), true);
    if (flag === 'NO_PATCH') noPatch.push(file);
  }
  const follow = noPatch.filter((f) => t.followup?.filter(f));
  if (follow.length) {
    const cmd = follow.map((f) => `echo '### ${f}'; gh api 'repos/${r}/contents/${f}?ref=${sha}' -H 'Accept: application/vnd.github.raw' | grep -n -F ${q(t.followup.grep)}`).join('; ');
    const g = rec.shell('raw | grep patchless', cmd);
    for (const block of g.stdout.split(/^### /m).slice(1)) { const [file, ...body] = block.split('\n'); rec.add(file, body.join('\n'), true); }
  }
  return summarize(rec, judge(rec, t.gt));
}

// Octocode helpers shared by both octocode arms.
// Inventory entries are either objects ({path,status,previousPath,patch,...}) or the
// compact form "S +a -d [!flag ...] name [<- previousFullPath]", optionally grouped
// as { "dir/": [entries] }.
const STATUS = { A: 'added', M: 'modified', D: 'removed', R: 'renamed', C: 'copied' };
export function parseInventory(list, dir = '') {
  const out = [];
  for (const item of list ?? []) {
    if (typeof item === 'string') {
      const m = /^([A-Z])\s+\+(\d+)\s+-(\d+)\s+((?:![A-Za-z]+\s+)*)(.+?)(?:\s+<-\s+(.+))?$/.exec(item);
      if (!m) continue;
      const flags = m[4].trim().split(/\s+/).filter(Boolean).map((f) => f.slice(1));
      out.push({ path: dir + m[5], status: STATUS[m[1]] ?? m[1], additions: +m[2], deletions: +m[3], flags, previousPath: m[6], patchUnavailable: flags.includes('tooLarge') || flags.some((f) => /nopatch|unavailable/i.test(f)) });
    } else if (item && typeof item === 'object' && typeof item.path === 'string') out.push({ ...item, path: dir + item.path });
    else if (item && typeof item === 'object') for (const [k, v] of Object.entries(item)) if (Array.isArray(v)) out.push(...parseInventory(v, dir + k));
  }
  return out;
}
function absorbPr(rec, e) {
  for (const pr of collect(e.sc, (o) => Array.isArray(o?.changedFiles))) {
    for (const f of parseInventory(pr.changedFiles)) {
      rec.add(f.path, `${f.status ?? ''} ${f.path} ${f.previousPath ?? ''}`, false);
      if (typeof f.patch === 'string') rec.add(f.path, f.patch, true);
    }
  }
}
function absorbFiles(rec, e, fallbackPath) {
  for (const f of collect(e.sc, (o) => typeof o?.content === 'string')) rec.add(f.path ?? fallbackPath, f.content, true);
}
async function prOrient(rec, c, t) {
  const base = { operation: 'pullRequest', owner: t.owner, repo: t.repo, number: t.number };
  const brief = { goal: t.question, reasoning: 'Orient on PR size and head SHA before choosing files.' };
  const meta = await octo(rec, c, 'metadata', 'ghGetHistoryItem', { ...base, ...brief });
  const pr = collect(meta.sc, (o) => o?.number === t.number && 'sourceSha' in o)[0] ?? {};
  const inventory = [];
  let e = await octo(rec, c, 'inventory p1', 'ghGetHistoryItem', { ...base, ...brief, reasoning: 'Patch-free inventory to pick candidate files.', content: { changedFiles: true } });
  for (let page = 1; e && page <= 40; page++) {
    for (const p of collect(e.sc, (o) => Array.isArray(o?.changedFiles))) inventory.push(...parseInventory(p.changedFiles));
    absorbPr(rec, e);
    const next = nextHints(e.sc).find((h) => /changedfilespage|nextfilepage/i.test(h.path));
    e = next ? await octo(rec, c, `inventory p${page + 1}`, next.tool, { queries: [next.query] }, { raw: true }) : null;
  }
  const seen = new Set();
  const files = inventory.filter((f) => f?.path && !seen.has(f.path) && seen.add(f.path));
  return { base, sha: pr.sourceSha, files, candidates: files.filter((f) => t.candidate(f.path, f)) };
}
async function prPatches(rec, c, t, base, files, extra = {}) {
  for (let i = 0; i < files.length; i += 100) {
    let e = await octo(rec, c, `selected patches (${files.slice(i, i + 100).length})`, 'ghGetHistoryItem', { ...base, goal: t.question, reasoning: 'Read only the candidate patches.', content: { patches: { mode: 'selected', files: files.slice(i, i + 100) } }, ...extra });
    for (let w = 1; e && w <= 60; w++) {
      absorbPr(rec, e);
      const next = nextHints(e.sc).find((h) => /continuepatch|nextpatch/i.test(h.path));
      e = next ? await octo(rec, c, `patch window ${w + 1}`, next.tool, { queries: [next.query] }, { raw: true }) : null;
    }
  }
}
const isPatchless = (f) => f.patchUnavailable || f.noPatch;

async function prOcto(c, t) {
  const rec = recorder();
  const { base, sha, candidates } = await prOrient(rec, c, t);
  const withPatch = candidates.filter((f) => !isPatchless(f) && !(t.metaOnly ?? []).includes(f.path)).map((f) => f.path);
  if (withPatch.length) await prPatches(rec, c, t, base, withPatch, { matchString: t.matchString });
  const follow = candidates.filter((f) => isPatchless(f) && t.followup?.filter(f.path));
  if (follow.length) {
    const e = await octo(rec, c, 'patchless: source match @sourceSha', 'ghGetFileContent', follow.map((f) => ({ owner: t.owner, repo: t.repo, path: f.path, branch: sha, matchString: t.followup.grep, contextLines: 0, goal: t.question, reasoning: 'GitHub omitted this patch; check the changed declaration at the head SHA.' })));
    absorbFiles(rec, e);
  }
  return { ...summarize(rec, judge(rec, t.gt)), candidates: candidates.length };
}

// Expert octocode: one filtered query per question, as gh-lean gets expert --jq filters.
// Each step is a pullRequest query fragment; continuations are followed verbatim.
async function prDirect(c, t) {
  const rec = recorder();
  const base = { operation: 'pullRequest', owner: t.owner, repo: t.repo, number: t.number };
  const steps = t.direct ?? [{ content: { patches: { mode: 'all' } }, matchString: t.matchString }];
  const inventory = [];
  let sha;
  for (const [i, step] of steps.entries()) {
    let e = await octo(rec, c, `direct ${i + 1}`, 'ghGetHistoryItem', { ...base, ...step, goal: t.question, reasoning: 'Ask the PR directly with filters.' });
    for (let w = 1; e && w <= 60; w++) {
      for (const p of collect(e.sc, (o) => Array.isArray(o?.changedFiles))) inventory.push(...parseInventory(p.changedFiles));
      sha ??= collect(e.sc, (o) => typeof o?.sourceSha === 'string')[0]?.sourceSha;
      absorbPr(rec, e);
      const next = nextHints(e.sc).find((h) => /continuepatch|nextpatch|changedfilespage|nextfilepage/i.test(h.path));
      e = next ? await octo(rec, c, `direct ${i + 1} p${w + 1}`, next.tool, { queries: [next.query] }, { raw: true }) : null;
    }
  }
  const follow = inventory.filter((f) => isPatchless(f) && t.followup?.filter(f.path));
  if (follow.length) {
    const e = await octo(rec, c, 'patchless: source match @sourceSha', 'ghGetFileContent', follow.map((f) => ({ owner: t.owner, repo: t.repo, path: f.path, branch: sha, matchString: t.followup.grep, contextLines: 0, goal: t.question, reasoning: 'GitHub omitted this patch; check the changed declaration at the head SHA.' })));
    absorbFiles(rec, e);
  }
  return summarize(rec, judge(rec, t.gt));
}

async function prClasify(c, t) {
  const rec = recorder();
  const { base, sha, candidates } = await prOrient(rec, c, t);
  const withPatch = candidates.filter((f) => !isPatchless(f) && !(t.metaOnly ?? []).includes(f.path)).map((f) => f.path);
  // Scout: one resource per candidate file (each captures 2 pages, so <=12 resources per matrix of 25 cells).
  const scores = new Map();
  const matrices = [];
  for (let i = 0; i < withPatch.length; i += 12) {
    const chunk = withPatch.slice(i, i + 12);
    matrices.push({ id: `m${matrices.length}`, goal: t.question, reasoning: 'Screen each changed file patch before reading any of them.',
      resources: chunk.map((f, j) => ({ id: `f${i + j}`, context: { tool: 'ghGetHistoryItem', query: { ...base, content: { patches: { mode: 'selected', files: [f] } } } } })),
      questions: [{ id: 'q', questionType: 'contribution', target: t.semantic }] });
  }
  // Send up to 5 matrices per call. When a matrix exceeds the 25-cell limit (large patches
  // capture several pages), follow the error hint: split its resources and retry.
  let unavailable = null;
  let queue = matrices;
  let round = 0;
  while (queue.length && round++ < 12) {
    const batch = queue.slice(0, 5);
    queue = queue.slice(5);
    const e = await octo(rec, c, `clasify scout (${batch.reduce((a, m) => a + m.resources.length, 0)} files)`, 'clasify', { queries: batch }, { raw: true });
    if (e.isError && !e.sc?.queries) { unavailable = e.text.slice(0, 200); break; }
    for (const qr of e.sc?.queries ?? []) {
      const m = batch.find((b) => b.id === qr.queryId);
      const over = (qr.resources ?? []).some((r) => (r.pages ?? []).some((pg) => pg.error?.code === 'classificationExpandedCellsExceeded'));
      if (over && m) {
        if (m.resources.length > 1) {
          const half = Math.ceil(m.resources.length / 2);
          queue.push({ ...m, id: `${m.id}a`, resources: m.resources.slice(0, half) }, { ...m, id: `${m.id}b`, resources: m.resources.slice(half) });
        } else scores.set(withPatch[+m.resources[0].id.slice(1)], null); // one file alone exceeds the cell budget
        continue;
      }
      for (const r of qr.resources ?? []) {
        const idx = +r.resourceId.slice(1);
        const vals = (r.pages ?? []).map((pg) => pg.answers?.q?.noul).filter((v) => typeof v === 'number');
        scores.set(withPatch[idx], vals.length ? Math.max(...vals) : null);
      }
    }
  }
  const pick = withPatch.filter((f) => (scores.get(f) ?? 0) >= CLASIFY_READ_MIN);
  if (pick.length) await prPatches(rec, c, t, base, pick);
  const follow = candidates.filter((f) => isPatchless(f) && t.followup?.filter(f.path));
  for (const f of follow) {
    const e = await octo(rec, c, `clasify locate ${path.basename(f.path)}`, 'clasify', { queries: [{ goal: t.question, reasoning: 'GitHub omitted this patch; locate the changed declaration without reading the file.',
      resources: [{ id: 'src', context: { tool: 'ghGetFileContent', query: { owner: t.owner, repo: t.repo, path: f.path, branch: sha, fullContent: true, minify: 'none' } }, prefilter: t.followup.prefilter }],
      questions: [{ id: 'loc', questionType: 'locate', target: t.followup.locate }] }] }, { raw: true });
    const best = e.sc?.queries?.[0]?.best?.loc?.[0];
    if (best && best.exists >= CLASIFY_READ_MIN) {
      const r = await octo(rec, c, `read located window ${path.basename(f.path)}`, 'ghGetFileContent', { owner: t.owner, repo: t.repo, path: f.path, branch: sha, startLine: Math.max(1, best.startLine - 5), endLine: best.endLine + 5, minify: 'none', goal: t.question, reasoning: 'Verify the located window.' });
      absorbFiles(rec, r, f.path);
    }
  }
  const scored = [...scores].map(([f, p]) => ({ f, p: p == null ? null : +p.toFixed(2), gt: !!t.gt[f] })).sort((a, b) => (b.p ?? -1) - (a.p ?? -1));
  return { ...summarize(rec, judge(rec, t.gt)), candidates: candidates.length, picked: pick.length, unscored: [...scores.values()].filter((v) => v == null).length, unavailable, scoreTop: scored.slice(0, 8), gtScores: scored.filter((s) => s.gt) };
}

async function runPr(c) {
  const out = [];
  for (const t of PR_TASKS.filter((x) => !ONLY || ONLY.includes(x.id))) {
    const api = prFiles(t);
    const sha = spawnSync('gh', ['pr', 'view', String(t.number), '-R', `${t.owner}/${t.repo}`, '--json', 'headRefOid', '--jq', '.headRefOid'], { encoding: 'utf8' }).stdout.trim();
    const row = { id: t.id, shape: t.shape, question: t.question, files: api.length, gtFiles: Object.keys(t.gt).length, gtProblems: verifyPrGt(t, api), arms: {} };
    const arms = { 'gh-typical': () => prGhTypical(t, api, sha), 'gh-lean': () => prGhLean(t, api, sha), octocode: () => prOcto(c, t), 'octocode-direct': () => prDirect(c, t), 'octocode+clasify': () => prClasify(c, t) };
    for (const [name, fn] of Object.entries(arms)) {
      if (ARMS && !ARMS.includes(name)) continue;
      try { row.arms[name] = await fn(); } catch (err) { row.arms[name] = { failed: String(err.stack ?? err).slice(0, 400) }; }
      const a = row.arms[name];
      console.log(`[pr] ${t.id} ${name.padEnd(17)} chars=${a.chars} calls=${a.calls} ms=${a.ms} recall=${a.recall} readP=${a.readPrecision} read=${a.filesRead} ok=${a.correct}${a.failed ? ' FAILED ' + a.failed : ''}`);
    }
    out.push(row);
  }
  return out;
}

// ================================================================= local suite
function freezeGt() {
  const gt = {};
  for (const t of LOCAL_TASKS.filter((x) => x.kind === 'symbol')) {
    const r = sh(`${RG} -l -w --no-messages ${q(t.symbol)} ${q(t.path)} | sort`, path.join(REPOS, t.repo));
    gt[t.id] = { commit: sh('git rev-parse HEAD', path.join(REPOS, t.repo)).stdout.trim(), usages: r.stdout.trim().split('\n').map((p) => path.normalize(p)) };
  }
  fs.writeFileSync(GT_FILE, JSON.stringify(gt, null, 1) + '\n');
  console.log(`froze ${Object.keys(gt).length} usage lists -> ${GT_FILE}`);
}
function verifyLocalGt(t, frozen) {
  const root = path.join(REPOS, t.repo);
  const problems = [];
  for (const [file, markers] of Object.entries(t.gt)) {
    const p = path.join(root, file);
    if (!fs.existsSync(p)) { problems.push(`${file} missing`); continue; }
    const text = fs.readFileSync(p, 'utf8');
    for (const m of markers) if (!text.includes(m)) problems.push(`${file} lacks marker ${m}`);
  }
  if (t.kind === 'symbol') {
    const f = frozen?.[t.id];
    if (!f) problems.push('no frozen usages (run --freeze-gt)');
    else {
      const head = sh('git rev-parse HEAD', root).stdout.trim();
      if (head !== f.commit) problems.push(`repo moved: ${head.slice(0, 10)} != frozen ${f.commit.slice(0, 10)}`);
    }
  }
  return problems;
}

const rel = (root, base, p) => path.relative(root, path.resolve(base ?? root, p));
function densestWindow(lines, size = 60) {
  const sorted = [...new Set(lines)].sort((a, b) => a - b);
  let best = [sorted[0], 1];
  for (let i = 0; i < sorted.length; i++) {
    let j = i; while (j + 1 < sorted.length && sorted[j + 1] < sorted[i] + size) j++;
    if (j - i + 1 > best[1]) best = [sorted[i], j - i + 1];
  }
  const start = Math.max(1, best[0] - 10);
  return [start, start + size - 1];
}
function rgHits(rec, out, root) {
  const hits = new Map();
  for (const line of out.split('\n')) {
    const m = /^(.+?):(\d+):(.*)$/.exec(line);
    if (!m) continue;
    const file = path.normalize(m[1]);
    rec.add(file, m[3], false);
    hits.set(file, [...(hits.get(file) ?? []), +m[2]]);
  }
  return hits;
}
function octoHits(rec, e, root) {
  const hits = new Map();
  const base = e.sc?.base;
  for (const f of collect(e.sc, (o) => typeof o?.path === 'string' && Array.isArray(o.matches))) {
    const file = rel(root, base, f.path);
    for (const m of f.matches) rec.add(file, m.value ?? '', false);
    const total = f.pagination?.totalMatches ?? f.matches.length;
    // A clipped file names its unshown hit lines (pagination.moreLines).
    const more = String(f.pagination?.moreLines ?? '').split(',').filter(Boolean).map(Number);
    hits.set(file, { lines: [...f.matches.map((m) => m.line), ...more], total });
  }
  return hits;
}
const topK = (entries, k) => entries.sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, k).map(([f]) => f);

function localShell(t, usages) {
  const rec = recorder();
  const root = path.join(REPOS, t.repo);
  const excl = (t.exclude ?? []).map((g) => `--glob ${q('!' + g)}`).join(' ');
  if (t.kind === 'symbol') {
    const d = rec.shell('rg definition', `${RG} -n --no-heading --color never -e ${q(t.defPattern)} ${q(t.path)}`, root);
    rgHits(rec, d.stdout, root);
    for (const [file, e] of rec.opened) e.read = true; // definition lines are the evidence
    const u = rec.shell('rg -l usages', `${RG} -l -w --color never ${q(t.symbol)} ${q(t.path)}`, root);
    rec.listed = new Set(u.stdout.trim().split('\n').filter(Boolean).map((p) => path.normalize(p)));
    return summarize(rec, judge(rec, t.gt, { usages }));
  }
  const s = rec.shell('rg search', `${RG} -n --no-heading --color never ${t.ci ? '-i' : ''} ${excl} -e ${q(t.pattern)} ${q(t.path)}`, root);
  const hits = rgHits(rec, s.stdout, root);
  const files = topK([...hits].map(([f, l]) => [f, l.length]), K_READ);
  if (files.length) {
    const wins = files.map((f) => [f, densestWindow(hits.get(f))]);
    const r = rec.shell(`sed read top ${files.length}`, wins.map(([f, [a, b]]) => `echo '### ${f}'; sed -n '${a},${b}p' ${q(f)}`).join('; '), root);
    for (const block of r.stdout.split(/^### /m).slice(1)) { const [file, ...body] = block.split('\n'); rec.add(file, body.join('\n'), true); }
  }
  return summarize(rec, judge(rec, t.gt));
}

async function localOcto(c, t, usages) {
  const rec = recorder();
  const root = path.join(REPOS, t.repo);
  const brief = { goal: t.question, reasoning: 'Discovery search before reading exact lines.' };
  const abs = path.join(root, t.path);
  if (t.kind === 'symbol') {
    const e = await octo(rec, c, 'localSearch def + usage files', 'localSearch', [
      { ...brief, path: abs, searchText: t.defPattern },
      { ...brief, path: abs, searchText: t.symbol, wholeWord: true, regex: 'literal', resultView: 'files' },
    ]);
    const r0 = { sc: { ...e.sc, results: [e.sc?.results?.[0]] } };
    octoHits(rec, r0, root);
    for (const [, v] of rec.opened) v.read = true;
    rec.listed = new Set((e.sc?.results?.[1]?.data?.files ?? []).map((f) => rel(root, e.sc?.base ?? abs, f.path)));
    return summarize(rec, judge(rec, t.gt, { usages }));
  }
  const e = await octo(rec, c, 'localSearch', 'localSearch', { ...brief, path: abs, searchText: t.pattern, ...(t.ci ? { caseMode: 'insensitive' } : {}), ...(t.exclude ? { exclude: t.exclude } : {}) });
  const hits = octoHits(rec, e, root);
  const files = topK([...hits].map(([f, h]) => [f, h.total]), K_READ);
  if (files.length) {
    const r = await octo(rec, c, `localFetch top ${files.length}`, 'localFetch', files.map((f) => { const [a, b] = densestWindow(hits.get(f).lines); return { goal: t.question, reasoning: 'Read the densest hit window.', path: path.join(root, f), startLine: a, endLine: b }; }));
    for (const [i, row] of (r.sc?.results ?? []).entries()) {
      const d = row?.data; if (typeof d?.content === 'string') rec.add(files[i], d.content, true);
    }
  }
  return summarize(rec, judge(rec, t.gt));
}

async function localClasify(c, t, usages) {
  const rec = recorder();
  const root = path.join(REPOS, t.repo);
  const abs = path.join(root, t.path);
  let questions = t.questions;
  let searchQuery = { path: abs, searchText: t.pattern, ...(t.ci ? { caseMode: 'insensitive' } : {}), ...(t.exclude ? { exclude: t.exclude } : {}) };
  if (t.kind === 'symbol') {
    const e = await octo(rec, c, 'localSearch usage files', 'localSearch', { goal: t.question, reasoning: 'List files referencing the symbol.', path: abs, searchText: t.symbol, wholeWord: true, regex: 'literal', resultView: 'files' });
    rec.listed = new Set((e.sc?.results?.[0]?.data?.files ?? []).map((f) => rel(root, e.sc?.base ?? abs, f.path)));
    questions = [`Where is ${t.symbol} declared (its definition, not a call site)?`];
    searchQuery = { path: abs, searchText: t.symbol, wholeWord: true, regex: 'literal' };
  }
  const e = await octo(rec, c, 'clasify locate over search', 'clasify', { queries: [{ goal: t.question, reasoning: 'Locate the deciding lines in unread search candidates before reading.',
    resources: [{ id: 's', context: { tool: 'localSearch', query: searchQuery, candidateEvidence: 'fileChunks' } }],
    questions: questions.map((target, i) => ({ id: `q${i}`, questionType: 'locate', target })) }] }, { raw: true });
  const unavailable = e.isError ? e.text.slice(0, 200) : null;
  // Documented read rule: take `best` (cross-page ranking) at exists >= 0.5, max 2 per
  // question; else the top page match. `best` names its file (path); older builds
  // without it are mapped back to their page by range.
  const qres = e.sc?.queries?.[0];
  const pages = qres?.resources?.[0]?.pages ?? [];
  const reads = new Map();
  const pageOf = (qid, b) => (b.path ? { source: { path: b.path } } : null) ?? pages.find((p) => (p.answers?.[qid]?.matches ?? []).some((m) => m.startLine === b.startLine && m.endLine === b.endLine));
  const addRead = (p, m) => { const file = rel(root, null, p.source.path); reads.set(`${file}:${m.startLine}`, { file, startLine: Math.max(1, m.startLine - 8), endLine: m.endLine + 8 }); };
  questions.forEach((_, i) => {
    const qid = `q${i}`;
    const best = (qres?.best?.[qid] ?? []).filter((b) => b.exists >= CLASIFY_READ_MIN).map((b) => [pageOf(qid, b), b]).filter(([p]) => p).slice(0, 2);
    if (best.length) { for (const [p, b] of best) addRead(p, b); return; }
    const ranked = pages.filter((p) => p.answers?.[qid]?.matches?.length).sort((a, b) => (b.answers[qid].exists ?? 0) - (a.answers[qid].exists ?? 0));
    if (ranked[0]) addRead(ranked[0], [...ranked[0].answers[qid].matches].sort((a, b) => b.probability - a.probability)[0]);
  });
  const list = [...reads.values()];
  if (list.length) {
    const r = await octo(rec, c, `localFetch ${list.length} located windows`, 'localFetch', list.map((w) => ({ goal: t.question, reasoning: 'Verify the located window.', path: path.join(root, w.file), startLine: w.startLine, endLine: w.endLine })));
    for (const [i, row] of (r.sc?.results ?? []).entries()) { const d = row?.data; if (typeof d?.content === 'string') rec.add(list[i].file, d.content, true); }
  }
  return { ...summarize(rec, judge(rec, t.gt, t.kind === 'symbol' ? { usages } : {})), unavailable, windows: list.map((w) => `${w.file}:${w.startLine}-${w.endLine}`) };
}

// Variant: the agent has run the discovery search (seen), then asks clasify to locate the
// answer inside the top-3 hit files (whole files, prefiltered by the matched strings)
// instead of reading hit windows. Not applicable to symbol tasks.
async function localClasifyFiles(c, t) {
  if (t.kind === 'symbol') return null;
  const rec = recorder();
  const root = path.join(REPOS, t.repo);
  const abs = path.join(root, t.path);
  const e = await octo(rec, c, 'localSearch', 'localSearch', { goal: t.question, reasoning: 'Discovery search before locating.', path: abs, searchText: t.pattern, ...(t.ci ? { caseMode: 'insensitive' } : {}), ...(t.exclude ? { exclude: t.exclude } : {}) });
  const hits = octoHits(rec, e, root);
  const files = topK([...hits].map(([f, h]) => [f, h.total]), K_READ);
  const re = new RegExp(t.pattern, t.ci ? 'gi' : 'g');
  const literals = [...new Set(collect(e.sc, (o) => typeof o?.value === 'string' && typeof o?.line === 'number').flatMap((m) => m.value.match(re) ?? []))].slice(0, 8);
  const resources = files.map((f, i) => ({ id: `f${i}`, context: { tool: 'localFetch', query: { path: path.join(root, f), fullContent: true, minify: 'none' } }, ...(literals.length ? { prefilter: literals } : {}) }));
  const matrix = (qs) => ({ goal: t.question, reasoning: 'Locate the deciding lines in the top hit files before reading them.', resources, questions: qs });
  const allQs = t.questions.map((target, i) => ({ id: `q${i}`, questionType: 'locate', target }));
  let cl = await octo(rec, c, 'clasify locate in top files', 'clasify', { queries: [matrix(allQs)] }, { raw: true });
  const overBudget = (x) => JSON.stringify(x.sc ?? {}).includes('classificationExpandedCellsExceeded');
  // Follow the cells-exceeded hint: one matrix per question, same call.
  if (overBudget(cl) && allQs.length > 1) cl = await octo(rec, c, 'clasify locate (split per question)', 'clasify', { queries: allQs.map((q) => ({ ...matrix([q]), id: q.id })) }, { raw: true });
  const bestFor = (qid) => (cl.sc?.queries ?? []).map((qq) => qq.best?.[qid]).find(Boolean) ?? [];
  const qres = cl.sc?.queries?.[0];
  const reads = new Map();
  t.questions.forEach((_, i) => {
    const qid = `q${i}`;
    let picks = bestFor(qid).filter((b) => b.exists >= CLASIFY_READ_MIN).slice(0, 2);
    if (!picks.length && bestFor(qid)[0]) picks = [bestFor(qid)[0]];
    for (const b of picks) { const file = files[+b.resourceId.slice(1)]; if (file) reads.set(`${file}:${b.startLine}`, { file, startLine: Math.max(1, b.startLine - 8), endLine: b.endLine + 8 }); }
  });
  const list = [...reads.values()];
  if (list.length) {
    const r = await octo(rec, c, `localFetch ${list.length} located windows`, 'localFetch', list.map((w) => ({ goal: t.question, reasoning: 'Verify the located window.', path: path.join(root, w.file), startLine: w.startLine, endLine: w.endLine })));
    for (const [i, row] of (r.sc?.results ?? []).entries()) { const d = row?.data; if (typeof d?.content === 'string') rec.add(list[i].file, d.content, true); }
  }
  return { ...summarize(rec, judge(rec, t.gt)), unavailable: cl.isError && !qres ? cl.text.slice(0, 200) : null, prefilter: literals, windows: list.map((w) => `${w.file}:${w.startLine}-${w.endLine}`) };
}

async function runLocal(c) {
  const frozen = fs.existsSync(GT_FILE) ? JSON.parse(fs.readFileSync(GT_FILE, 'utf8')) : {};
  const out = [];
  for (const t of LOCAL_TASKS.filter((x) => !ONLY || ONLY.includes(x.id))) {
    const usages = frozen[t.id]?.usages;
    const row = { id: t.id, lang: t.lang, kind: t.kind, question: t.question, gtFiles: Object.keys(t.gt).length, usages: usages?.length, gtProblems: verifyLocalGt(t, frozen), arms: {} };
    const arms = { 'rg/sed': () => localShell(t, usages), octocode: () => localOcto(c, t, usages), 'octocode+clasify': () => localClasify(c, t, usages), 'octocode+clasify(files)': () => localClasifyFiles(c, t) };
    for (const [name, fn] of Object.entries(arms)) {
      if (ARMS && !ARMS.includes(name)) continue;
      try { row.arms[name] = await fn(); } catch (err) { row.arms[name] = { failed: String(err.stack ?? err).slice(0, 400) }; }
      const a = row.arms[name];
      if (a === null) { delete row.arms[name]; continue; }
      console.log(`[local] ${t.id.padEnd(12)} ${name.padEnd(17)} chars=${a.chars} calls=${a.calls} ms=${a.ms} recall=${a.recall}${a.usageRecall != null ? ` uR=${a.usageRecall} uP=${a.usagePrecision}` : ''} readP=${a.readPrecision} ok=${a.correct}${a.failed ? ' FAILED ' + a.failed : ''}`);
    }
    out.push(row);
  }
  return out;
}

// ================================================================= report
function totals(rows) {
  const t = {};
  for (const r of rows) for (const [arm, a] of Object.entries(r.arms)) {
    const x = (t[arm] ??= { tasks: 0, correct: 0, chars: 0, calls: 0, ms: 0, filesRead: 0, recall: 0 });
    x.tasks++; x.correct += a.correct ? 1 : 0; x.chars += a.chars ?? 0; x.calls += a.calls ?? 0; x.ms += a.ms ?? 0; x.filesRead += a.filesRead ?? 0; x.recall += a.recall ?? 0;
  }
  for (const x of Object.values(t)) { x.tokens = Math.round(x.chars / 4); x.recall = +(x.recall / x.tasks).toFixed(2); }
  return t;
}
const fmt = (n) => (n == null ? '—' : n >= 10000 ? `${(n / 1000).toFixed(0)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));
function table(rows, extra) {
  const lines = ['| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |', '|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|'];
  for (const r of rows) for (const [arm, a] of Object.entries(r.arms)) {
    if (a.failed) { lines.push(`| ${r.id} | ${arm} | failed | | | | | | | ✗ |`); continue; }
    const rc = a.usageRecall != null ? `${a.recall} (usages R ${a.usageRecall} / P ${a.usagePrecision})` : a.recall;
    lines.push(`| ${r.id} | ${arm} | ${fmt(a.chars)} | ${fmt(a.tokens)} | ${a.calls} | ${fmt(a.ms)} | ${rc} | ${a.readPrecision} | ${a.filesRead} | ${a.correct ? '✓' : '✗'} |`);
  }
  lines.push('', '| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |', '|---|--:|--:|--:|--:|--:|--:|--:|');
  for (const [arm, x] of Object.entries(totals(rows))) lines.push(`| ${arm} | ${x.correct}/${x.tasks} | ${fmt(x.chars)} | ${fmt(x.tokens)} | ${x.calls} | ${fmt(x.ms)} | ${x.recall} | ${x.filesRead} |`);
  return lines.join('\n');
}

// The MCP refuses to start while core and native contracts drift (e.g. mid-rebuild).
// Probe with a short initialize and wait instead of hanging on the 240 s call timeout.
async function waitForMcp(maxMinutes = +(arg('wait-mcp', '20'))) {
  const entry = path.resolve(HERE, '../../packages/octocode-mcp/dist/index.js');
  const init = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'bench-probe', version: '1' } } });
  const deadline = Date.now() + maxMinutes * 60_000;
  for (;;) {
    const r = spawnSync(process.execPath, [entry], { input: init + '\n', encoding: 'utf8', timeout: 20_000, env: { ...process.env, ENABLE_LOCAL: 'true' } });
    const out = (r.stdout ?? '') + (r.stderr ?? '');
    if (/"result"/.test(out)) return;
    if (Date.now() > deadline) throw new Error(`MCP not ready after ${maxMinutes} min: ${out.slice(0, 300)}`);
    console.error(`[bench] MCP not ready (${out.slice(0, 120).replace(/\n/g, ' ')}); retrying in 30 s`);
    await new Promise((res) => setTimeout(res, 30_000));
  }
}

async function main() {
  if (argv.includes('--freeze-gt')) return freezeGt();
  fs.mkdirSync(RESULTS, { recursive: true });
  const needsMcp = !ARMS || ARMS.some((a) => a.startsWith('octocode'));
  if (needsMcp) await waitForMcp();
  const c = needsMcp ? await startServer({ env: { ENABLE_LOCAL: 'true' } }) : { close() {} };
  const result = { label: LABEL, at: new Date().toISOString(), node: process.version, rg: RG, suites: {} };
  try {
    if (SUITE === 'pr' || SUITE === 'all') result.suites.pr = await runPr(c);
    if (SUITE === 'local' || SUITE === 'all') result.suites.local = await runLocal(c);
  } finally { c.close(); }
  for (const [name, rows] of Object.entries(result.suites)) result[`${name}Totals`] = totals(rows);
  const json = path.join(RESULTS, `bench-${LABEL}.json`);
  fs.writeFileSync(json, JSON.stringify(result, null, 1));
  const md = [`# bench ${LABEL} (${result.at})`, ''];
  for (const [name, rows] of Object.entries(result.suites)) {
    md.push(`## ${name} suite`, '', table(rows), '');
    const bad = rows.filter((r) => r.gtProblems?.length);
    if (bad.length) md.push('Ground-truth problems:', ...bad.map((r) => `- ${r.id}: ${r.gtProblems.join('; ')}`), '');
  }
  fs.writeFileSync(path.join(RESULTS, `bench-${LABEL}.md`), md.join('\n'));
  console.log(md.join('\n'));
  console.log(`\nwrote ${json}`);
}
await main();
