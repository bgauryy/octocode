// GitHub tools, data fidelity: every returned byte, range, file list, diff
// stat, and filter is proven against the local clone pinned to the same SHA
// (`git` is the ground truth, never the tool), plus cross-tool agreement
// (search → read, commit ↔ PR ↔ compare) and clasify over GitHub resources.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, checks, collect, inventoryRows, rowData, sourceView, startServer, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('github');
const client = await startServer();
const { call, raw } = client;

const git = (dir, ...args) => execFileSync('git', ['-C', path.join(REPOS, dir), ...args], { encoding: 'utf8', maxBuffer: 256 << 20 });
const REPO_DIRS = ['rust', 'c', 'python', 'go', 'typescript'];
const pinned = Object.fromEntries(REPO_DIRS.map(dir => {
  const [, owner, repo] = git(dir, 'remote', 'get-url', 'origin').trim().match(/github\.com\/([^/]+)\/(.+?)(?:\.git)?$/);
  return [dir, { dir, owner, repo, sha: git(dir, 'rev-parse', 'HEAD').trim() }];
}));
const local = (dir, file) => fs.readFileSync(path.join(REPOS, dir, file), 'utf8');
const localLines = (dir, file) => local(dir, file).replace(/\n$/, '').split('\n');
const fileRow = entry => rowData(entry)?.files?.[0];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
let rateLimitWaits = 0;
/** GitHub search that waits out GitHub's code-search quota (10/min) instead of reading an error row as "no results". */
const SEARCH_TOOL = { code: 'ghSearchCode', tree: 'ghStructure', repositories: 'ghSearchRepo' };
async function search(query, tool) {
  // Legacy `operation` selects the split tool (ghSearch → ghSearchCode/ghStructure/ghSearchRepo);
  // a `next.*` query is replayed verbatim with its own tool.
  const { operation, ...rest } = query;
  for (let attempt = 0; attempt < 6; attempt++) {
    const out = tool ? await raw(tool, query) : await call(SEARCH_TOOL[operation], rest);
    const data = rowData(out);
    if (data?.errorCode !== 'rateLimited') return out;
    rateLimitWaits += 1;
    await sleep(((data.retryAfterSeconds ?? data.rateLimit?.retryAfterSeconds ?? 10) + 1) * 1000);
  }
  throw new Error(`still rate limited: ${JSON.stringify(query)}`);
}
const stats = { bytesCompared: 0, rangesCompared: 0, snippetsChecked: 0 };

// Sources per repo: one mid-size file (whole read) and the largest tracked
// source file (walked in windows to the end).
const SOURCES = {
  rust: { small: 'tokio/src/sync/oneshot.rs', large: 'tokio/src/sync/mpsc/bounded.rs', keywords: ['try_recv', 'Semaphore', 'spawn_blocking'] },
  c: { small: 'src/adlist.c', large: 'src/server.c', keywords: ['serverCron', 'listCreate', 'dictScan'] },
  python: { small: 'django/db/models/aggregates.py', large: 'django/db/models/query.py', keywords: ['_fetch_all', 'select_related', 'bulk_create'] },
  go: { small: 'tsdb/chunks/chunks.go', large: 'tsdb/db.go', keywords: ['NewHead', 'Appender', 'Compact'] },
  typescript: { small: 'packages/typescript/src/ast/astnav.ts', large: 'packages/typescript/src/api/sync/api.ts', keywords: ['registerModuleResolutionCallback', 'isUnionType', 'createSnapshot'] },
};

// ── A. ghGetFileContent fidelity ───────────────────────────────────────────
for (const r of Object.values(pinned)) {
  const src = SOURCES[r.dir];
  for (const file of [src.small, src.large]) {
    if (!fs.existsSync(path.join(REPOS, r.dir, file))) { check(`${r.dir}: ${file} exists locally`, false); continue; }
    const text = local(r.dir, file);
    const lines = localLines(r.dir, file);
    const base = { owner: r.owner, repo: r.repo, branch: r.sha, path: file };
    // Whole file when it fits; otherwise the tool must say why and offer a way on.
    const whole = await call('ghGetFileContent', { ...base, fullContent: true, debug: true });
    const w = fileRow(whole);
    check(`${r.dir}/${file}: totalLines and sourceBytes match git`, w?.totalLines === lines.length && w?.sourceBytes === Buffer.byteLength(text), `tool ${w?.totalLines}L/${w?.sourceBytes}B vs git ${lines.length}L/${Buffer.byteLength(text)}B`);
    if (!w?.isPartial) {
      const whole = sourceView(w).text;
      check(`${r.dir}/${file}: fullContent is byte-identical`, whole === text || whole === text.replace(/\n$/, ''), firstDiff(whole, text));
      stats.bytesCompared += Buffer.byteLength(text);
    } else {
      // Oversize fullContent returns page 1 inline (exact source) plus a continuation.
      const firstLines = sourceView(w).text.replace(/\n$/, '');
      const inline = firstLines.length > 0 && text.startsWith(firstLines) && (w.partialReasons ?? []).length > 0 && !!w.next;
      check(`${r.dir}/${file}: oversize fullContent returns an exact first page and a continuation`, inline, JSON.stringify({ partialReasons: w.partialReasons, bytes: firstLines.length, next: Object.keys(w.next ?? {}) }));
    }
    // Walk the file in 1500-line windows to the end; every returned range must equal git.
    let next = 1;
    let covered = 0;
    let mismatch = null;
    for (let guard = 0; next <= lines.length && guard < 40; guard++) {
      const end = Math.min(lines.length, next + 1499);
      const out = await call('ghGetFileContent', { ...base, startLine: next, endLine: end });
      const row = fileRow(out);
      const view = sourceView(row);
      const ranges = view.ranges;
      const got = view.text.replace(/\n$/, '');
      const want = ranges.map(g => lines.slice(g.start - 1, g.end).join('\n')).join('\n');
      if (!ranges.length || got !== want) { mismatch = `window ${next}-${end}: ${firstDiff(got, want)}`; break; }
      covered += ranges.reduce((n, g) => n + g.end - g.start + 1, 0);
      stats.bytesCompared += Buffer.byteLength(got);
      next = ranges.at(-1).end + 1;
    }
    check(`${r.dir}/${file}: line windows walked to the end are identical to git`, !mismatch && covered === lines.length, mismatch ?? `${covered}/${lines.length} lines`);
  }
  // Random exact ranges, including the last line and single-line reads.
  const file = src.large;
  const lines = localLines(r.dir, file);
  let bad = [];
  const picks = [[1, 1], [lines.length, lines.length], [lines.length - 5, lines.length]];
  for (let i = 0; i < 12; i++) { const s = 1 + Math.floor(((i * 7919) % 997) / 997 * (lines.length - 40)); picks.push([s, s + (i % 5) * 9]); }
  for (const [s, e] of picks) {
    const row = fileRow(await call('ghGetFileContent', { owner: r.owner, repo: r.repo, branch: r.sha, path: file, startLine: s, endLine: e }));
    const got = sourceView(row).text.replace(/\n$/, '');
    if (got !== lines.slice(s - 1, e).join('\n')) bad.push(`${s}-${e}`);
    stats.rangesCompared += 1;
  }
  check(`${r.dir}: ${picks.length} exact ranges (first/last/single line) equal git`, bad.length === 0, bad.join(','));
  // matchString windows: each returned range is exact and every literal hit is covered.
  const needle = { rust: 'pub fn try_', c: 'serverCron', python: 'def _', go: 'func (db *DB)', typescript: 'export function is' }[r.dir];
  const m = fileRow(await call('ghGetFileContent', { owner: r.owner, repo: r.repo, branch: r.sha, path: file, matchString: needle, matchStringCaseSensitive: true, contextLines: 0 }));
  const hitLines = lines.map((l, i) => l.includes(needle) ? i + 1 : 0).filter(Boolean);
  const mView = sourceView(m);
  const ranges = mView.ranges;
  const inRanges = hitLines.filter(n => ranges.some(g => g.start <= n && n <= g.end));
  // One block per numbered range: a run of single lines shares one gap marker.
  const textLines = mView.text.replace(/\n$/, '').split('\n').filter(l => !/^\.\.\. \[.* omitted\] \.\.\.$/.test(l));
  const blocks = [];
  for (const g of ranges) blocks.push(textLines.splice(0, g.end - g.start + 1).join('\n'));
  const exact = textLines.length === 0 && ranges.every((g, i) => blocks[i] === lines.slice(g.start - 1, g.end).join('\n'));
  check(`${r.dir}: matchString "${needle}" covers every literal hit (${hitLines.length}) with exact windows`, inRanges.length === hitLines.length && exact || (m?.isPartial && exact && inRanges.length > 0), `hits=${hitLines.length} covered=${inRanges.length} ranges=${ranges.length} blocks=${blocks.length} partial=${!!m?.isPartial}`);
}

// Byte chunks on an ASCII file reassemble to the exact bytes.
{
  const r = pinned.rust;
  const file = SOURCES.rust.small;
  const text = local(r.dir, file);
  let offset = 0;
  let joined = '';
  for (let guard = 0; guard < 60 && offset < text.length; guard++) {
    const row = fileRow(await call('ghGetFileContent', { owner: r.owner, repo: r.repo, branch: r.sha, path: file, chunkType: 'bytes', offset, chunkSize: 4000 }));
    const piece = row?.content ?? '';
    if (!piece) break;
    joined += piece;
    offset += Buffer.byteLength(piece);
  }
  check('byte chunks reassemble the file exactly', joined === text, firstDiff(joined, text));
}

// ── B. ghSearch fidelity ───────────────────────────────────────────────────
for (const r of Object.values(pinned)) {
  let files = 0, contains = 0, snippets = 0, snippetsFound = 0, missing = [];
  for (const keyword of SOURCES[r.dir].keywords) {
    const out = await search({ operation: 'code', owner: r.owner, repo: r.repo, keywords: [keyword], pageSize: 10 });
    for (const f of rowData(out)?.files ?? []) {
      files += 1;
      const full = path.join(REPOS, r.dir, f.path);
      if (!fs.existsSync(full)) { missing.push(f.path); continue; }
      const body = fs.readFileSync(full, 'utf8');
      if (body.toLowerCase().includes(keyword.toLowerCase())) contains += 1;
      for (const s of f.matches ?? []) {
        snippets += 1;
        const core = s.value.split('\n').map(l => l.trimEnd()).filter(l => l.trim()).join('\n');
        const hay = body.split('\n').map(l => l.trimEnd()).filter(l => l.trim()).join('\n');
        if (hay.includes(core)) snippetsFound += 1;
      }
    }
  }
  stats.snippetsChecked += snippets;
  check(`${r.dir}: code search precision — returned files contain the keyword`, files > 0 && contains >= files - missing.length && contains / files >= 0.9, `${contains}/${files} (missing locally: ${missing.slice(0, 3).join(',')})`);
  check(`${r.dir}: code search snippets are verbatim source`, snippets > 0 && snippetsFound / snippets >= 0.9, `${snippetsFound}/${snippets}`);
}
// Pagination: every page is distinct, totals are consistent, next.page walks.
{
  const r = pinned.rust;
  let query = { operation: 'code', owner: r.owner, repo: r.repo, keywords: ['Semaphore'], pageSize: 10 };
  let tool;
  const seen = new Set();
  let dupes = 0, pages = 0, total = null;
  while (query && pages < 6) {
    const out = await search(query, tool);
    const data = rowData(out);
    pages += 1;
    total ??= data?.pagination?.totalMatches;
    for (const f of data?.files ?? []) { const key = f.path; if (seen.has(key)) dupes += 1; seen.add(key); }
    query = data?.next?.nextPage?.query;
    tool = data?.next?.nextPage?.tool;
  }
  check('code search pagination: no file repeats across pages', dupes === 0 && pages > 1, `pages=${pages} files=${seen.size} dupes=${dupes} total=${total}`);
}
// Filters are honored on every row.
{
  const r = pinned.rust;
  const ext = rowData(await search({ operation: 'code', owner: r.owner, repo: r.repo, keywords: ['Semaphore'], extension: 'rs', pageSize: 20 }))?.files ?? [];
  check('extension filter: every path ends in .rs', ext.length > 0 && ext.every(f => f.path.endsWith('.rs')), ext.filter(f => !f.path.endsWith('.rs')).map(f => f.path).join(','));
  const pre = rowData(await search({ operation: 'code', owner: r.owner, repo: r.repo, keywords: ['Semaphore'], path: 'tokio/src/sync', pageSize: 20 }))?.files ?? [];
  check('path filter: every path is under tokio/src/sync', pre.length > 0 && pre.every(f => f.path.startsWith('tokio/src/sync')), pre.filter(f => !f.path.startsWith('tokio/src/sync')).map(f => f.path).join(','));
  const fname = rowData(await search({ operation: 'code', owner: r.owner, repo: r.repo, keywords: ['impl'], filename: 'bounded.rs', pageSize: 20 }))?.files ?? [];
  check('filename qualifier: every basename contains bounded.rs and the exact file is listed', fname.length > 0 && fname.every(f => path.basename(f.path).includes('bounded.rs')) && fname.some(f => f.path === 'tokio/src/sync/mpsc/bounded.rs'), fname.map(f => f.path).join(','));
  const byPath = rowData(await search({ operation: 'code', owner: r.owner, repo: r.repo, keywords: ['semaphore'], match: 'path', pageSize: 20 }))?.files ?? [];
  check('match:path: every returned path names the keyword', byPath.length > 0 && byPath.every(f => /semaphore/i.test(f.path)), byPath.filter(f => !/semaphore/i.test(f.path)).map(f => f.path).join(','));
}
// Tree listing equals git ls-tree at the pinned SHA (files and folders).
for (const r of Object.values(pinned)) {
  const dir = path.dirname(SOURCES[r.dir].large);
  const listed = new Set();
  let data;
  let treeQuery = { operation: 'tree', owner: r.owner, repo: r.repo, branch: r.sha, path: dir, maxDepth: 1, pageSize: 200 };
  let treeTool;
  for (let page = 0; treeQuery && page < 10; page++) {
    data = rowData(await search(treeQuery, treeTool));
    for (const node of data?.structure ?? []) {
      for (const f of node.files ?? []) listed.add(node.dir === '.' ? f : `${node.dir}/${f}`);
      for (const d of node.folders ?? node.dirs ?? []) listed.add(`${node.dir === '.' ? '' : `${node.dir}/`}${d}/`);
    }
    const pageHint = data?.pagination?.hasMore ? Object.values(data?.next ?? {}).find(n => n?.query?.page) : null;
    treeQuery = pageHint?.query;
    treeTool = pageHint?.tool;
  }
  const truth = new Set(git(r.dir, 'ls-tree', '--name-only', `HEAD:${dir}`).trim().split('\n').filter(Boolean)
    .map(n => git(r.dir, 'cat-file', '-t', `HEAD:${dir}/${n}`).trim() === 'tree' ? `${n}/` : n));
  const truthFiles = [...truth].filter(n => !n.endsWith('/'));
  const listedFiles = [...listed].filter(n => !n.endsWith('/'));
  const missingFiles = truthFiles.filter(n => !listed.has(n));
  const extra = listedFiles.filter(n => !truth.has(n));
  check(`${r.dir}: tree ${dir} lists exactly git's files`, missingFiles.length === 0 && extra.length === 0 && (data?.commitSha ?? data?.resolvedBranch ?? r.sha) === r.sha, `missing=${missingFiles.slice(0, 5)} extra=${extra.slice(0, 5)} resolved=${(data?.commitSha ?? data?.resolvedBranch)?.slice(0, 8)}`);
}
// Repository search filters (hoisted `shared` fields apply to every row).
{
  const out = await search({ operation: 'repositories', owner: 'tokio-rs', stars: '>3000', pageSize: 10 });
  const shared = out.sc?.shared ?? {};
  const rows = (rowData(out)?.repositories ?? []).map(row => ({ ...shared, ...row }));
  // Rows name the repository as `owner/name`.
  check('repositories: owner and stars filters hold on every row', rows.length > 0 && rows.every(x => x.repo?.split('/')[0] === 'tokio-rs' && x.stars > 3000), rows.map(x => `${x.repo}:${x.stars}`).join(','));
  const lang = await search({ operation: 'repositories', keywords: ['async runtime'], language: 'rust', stars: '>1000', pageSize: 10 });
  const langRows = (rowData(lang)?.repositories ?? []).map(row => ({ ...(lang.sc?.shared ?? {}), ...row }));
  check('repositories: language filter holds on every row', langRows.length > 0 && langRows.every(x => /rust/i.test(x.language ?? '')), langRows.map(x => `${x.repo}:${x.language}`).join(','));
}

// ── C/D. History: commit ↔ git, PR ↔ commit, compare ↔ commit ─────────────
{
  const r = pinned.rust;
  const parent = git(r.dir, 'rev-parse', 'HEAD^').trim();
  const numstat = git(r.dir, 'diff', '--numstat', parent, 'HEAD').trim().split('\n').map(l => { const [a, d, f] = l.split('\t'); return { f, a: +a, d: +d }; });
  // `parents` is metadata (debug-only); the exact SHA equality needs it.
  const commit = rowData(await call('ghGetHistoryItem', { operation: 'commit', owner: r.owner, repo: r.repo, ref: r.sha, debug: true }));
  check('commit: message equals git', (commit?.message ?? '').trim() === git(r.dir, 'log', '-1', '--format=%B').trim(), commit?.message?.slice(0, 80));
  check('commit: author name/email and parents equal git', commit?.author?.name === git(r.dir, 'log', '-1', '--format=%an').trim() && commit?.author?.email === git(r.dir, 'log', '-1', '--format=%ae').trim() && JSON.stringify(commit?.parents) === JSON.stringify([parent]), JSON.stringify({ a: commit?.author, p: commit?.parents }));
  // Commit file rows: `path` + `stat` ("M +3 -1"), like PR patch rows.
  const fileRow = f => { const [, a, d] = (f?.stat ?? '').match(/\+(\d+) -(\d+)/) ?? []; return { path: f?.path ?? f?.filename, additions: a === undefined ? f?.additions : +a, deletions: d === undefined ? f?.deletions : +d }; };
  const files = (commit?.files ?? []).map(fileRow);
  check('commit: changed files and +/- counts equal git numstat', files.length === numstat.length && numstat.every(n => files.some(f => f.path === n.f && f.additions === n.a && f.deletions === n.d)), JSON.stringify(files) + ' vs ' + JSON.stringify(numstat));
  const withDiff = rowData(await call('ghGetHistoryItem', { operation: 'commit', owner: r.owner, repo: r.repo, ref: r.sha, includeDiff: true }));
  const patchOk = numstat.every(n => {
    const f = (withDiff?.files ?? []).find(x => fileRow(x).path === n.f);
    const gitPatch = git(r.dir, 'diff', parent, 'HEAD', '--', n.f).split('\n').filter(l => /^[+-](?![+-])/.test(l));
    const toolPatch = (f?.patch ?? '').split('\n').filter(l => /^[+-](?![+-])/.test(l));
    return f && JSON.stringify(toolPatch) === JSON.stringify(gitPatch);
  });
  check('commit includeDiff: +/- lines equal git diff', patchOk);
  const compare = rowData(await call('ghGetHistoryItem', { operation: 'compare', owner: r.owner, repo: r.repo, base: parent, head: r.sha }));
  const cmpFiles = (compare?.files ?? []).map(f => fileRow(f).path);
  check('compare parent...HEAD: same files as the commit', cmpFiles.length === numstat.length && numstat.every(n => cmpFiles.includes(n.f)), cmpFiles.join(','));
  const prNumber = +(git(r.dir, 'log', '-1', '--format=%s').match(/#(\d+)\)/)?.[1] ?? 0);
  if (prNumber) {
    const pr = rowData(await call('ghGetHistoryItem', { operation: 'pullRequest', owner: r.owner, repo: r.repo, number: prNumber, content: { changedFiles: true } }))?.pullRequests?.[0];
    check(`PR #${prNumber}: mergeCommitSha is the pinned squash commit`, pr?.mergeCommitSha === r.sha && !pr?.hints?.getMergeCommit, `mergeCommitSha=${pr?.mergeCommitSha}`);
    // A squash headline `… (#N)` routes straight to hints.readPullRequest; otherwise hints.findPullRequest searches by SHA.
    const prHint = commit?.hints?.readPullRequest ?? commit?.hints?.findPullRequest;
    check(`commit ${r.sha.slice(0, 8)}: PR continuation exists`, !!prHint);
    const found = prHint ? rowData(await raw(prHint.tool, prHint.query)) : null;
    check(`commit ${r.sha.slice(0, 8)}: hints.readPullRequest/findPullRequest reaches PR #${prNumber}`, collect(found, o => o.number === prNumber && typeof o.title === 'string').length > 0, JSON.stringify(prHint?.query));
    check(`PR #${prNumber}: merged, and its changed files/+/- equal the squash commit`, pr?.state === 'merged' && pr?.additions === numstat.reduce((s, n) => s + n.a, 0) && pr?.deletions === numstat.reduce((s, n) => s + n.d, 0) && numstat.every(n => inventoryRows(pr.changedFiles).some(f => f.path === n.f && f.additions === n.a && f.deletions === n.d)), JSON.stringify({ state: pr?.state, add: pr?.additions, del: pr?.deletions }));
    const patches = rowData(await call('ghGetHistoryItem', { operation: 'pullRequest', owner: r.owner, repo: r.repo, number: prNumber, content: { patches: { mode: 'all' } } }))?.pullRequests?.[0];
    const prPatchOk = numstat.every(n => {
      const f = collect(patches, o => (o.path === n.f || o.filename === n.f) && typeof o.patch === 'string')[0];
      const gitPatch = git(r.dir, 'diff', parent, 'HEAD', '--', n.f).split('\n').filter(l => /^[+-](?![+-])/.test(l));
      return f && JSON.stringify(f.patch.split('\n').filter(l => /^[+-](?![+-])/.test(l))) === JSON.stringify(gitPatch);
    });
    check(`PR #${prNumber}: patches equal git diff of the squash commit`, prPatchOk);
  }
  // Commit search: HEAD is listed (or superseded by newer upstream commits), rows resolve.
  const list = rowData(await call('ghSearchHistory', { operation: 'commit', owner: r.owner, repo: r.repo, pageSize: 20 }));
  const shas = collect(list, o => typeof o.sha === 'string').map(o => o.sha);
  check('commit search: the pinned HEAD appears in recent history', shas.includes(r.sha), `${shas.length} shas; head ${r.sha.slice(0, 8)}`);
  const touched = rowData(await call('ghSearchHistory', { operation: 'commit', owner: r.owner, repo: r.repo, path: 'tokio/src/signal/registry.rs', pageSize: 3 }));
  let pathOk = 0, pathRows = collect(touched, o => typeof o.sha === 'string').slice(0, 3);
  for (const row of pathRows) {
    const c = rowData(await call('ghGetHistoryItem', { operation: 'commit', owner: r.owner, repo: r.repo, ref: row.sha }));
    if ((c?.files ?? []).some(f => fileRow(f).path === 'tokio/src/signal/registry.rs')) pathOk += 1;
  }
  check('commit search path filter: every commit touches the path', pathRows.length > 0 && pathOk === pathRows.length, `${pathOk}/${pathRows.length}`);
  const ranged = rowData(await call('ghSearchHistory', { operation: 'commit', owner: r.owner, repo: r.repo, since: '2026-09-01', until: '2026-09-20', pageSize: 10 }));
  const dates = collect(ranged, o => typeof o.sha === 'string').map(o => o.date ?? o.author?.date ?? o.committer?.date ?? o.committedDate);
  check('commit search since/until: every date inside the range', dates.length > 0 && dates.every(d => d && d >= '2026-09-01' && d <= '2026-09-20T23:59:59Z'), dates.join(','));
  const merged = rowData(await call('ghSearchHistory', { operation: 'pullRequest', owner: r.owner, repo: r.repo, state: 'merged', author: 'Darksonn', pageSize: 10 }));
  const prRows = collect(merged, o => typeof o.number === 'number' && typeof o.title === 'string');
  check('PR search state+author: every row merged and by the author', prRows.length > 0 && prRows.every(p => p.state === 'merged' && (p.author === 'Darksonn' || p.author?.login === 'Darksonn')), JSON.stringify(prRows.filter(p => !(p.state === 'merged' && (p.author === 'Darksonn' || p.author?.login === 'Darksonn'))).slice(0, 2)));
  const labeled = rowData(await call('ghSearchHistory', { operation: 'pullRequest', owner: r.owner, repo: r.repo, label: ['M-signal'], pageSize: 10 }));
  const lrows = collect(labeled, o => typeof o.number === 'number' && typeof o.title === 'string');
  check('PR search label: every row carries the label', lrows.length > 0 && lrows.every(p => (p.labels ?? []).map(l => l.name ?? l).includes('M-signal')), JSON.stringify(lrows.filter(p => !(p.labels ?? []).map(l => l.name ?? l).includes('M-signal')).slice(0, 2)));
  const closed = rowData(await call('ghSearchHistory', { operation: 'issue', owner: r.owner, repo: r.repo, state: 'closed', created: '2026-01-01..2026-06-30', pageSize: 10 }));
  const irows = collect(closed, o => typeof o.number === 'number' && typeof o.title === 'string');
  check('issue search state+created: every row closed and created in range', irows.length > 0 && irows.every(i => i.state === 'closed' && (i.createdAt ?? '') >= '2026-01-01' && (i.createdAt ?? '') <= '2026-06-30T23:59:59Z'), JSON.stringify(irows.filter(i => !(i.state === 'closed' && (i.createdAt ?? '') >= '2026-01-01' && (i.createdAt ?? '') <= '2026-06-30T23:59:59Z')).slice(0, 2)));
}

// ── E. clasify over GitHub resources (ground truth from the pinned clone) ──
{
  const LOCATE = [
    { dir: 'rust', file: 'tokio/src/sync/mpsc/bounded.rs', re: /^pub fn channel</, target: 'The public function that creates a bounded multi-producer channel with a given buffer capacity.' },
    { dir: 'c', file: 'src/server.c', re: /^int serverCron\(/, target: 'The periodic timer function that runs background housekeeping tasks many times per second.' },
    { dir: 'python', file: 'django/db/models/query.py', re: /def _fetch_all\(/, target: 'The method that evaluates the query and fills the result cache.' },
    { dir: 'go', file: 'tsdb/db.go', re: /^func Open\(/, target: 'The exported function that opens an existing database directory or creates a new one.' },
  ];
  const rows = [];
  for (const t of LOCATE) {
    const r = pinned[t.dir];
    const lines = localLines(r.dir, t.file);
    const bytes = Buffer.byteLength(local(r.dir, t.file));
    let request = {
      mainGoal: 'Locate a declaration in an unread GitHub file',
      reasoning: 'Locate in an unread GitHub file.',
      resources: [{ id: 'gh', context: { tool: 'ghGetFileContent', query: { reasoning: 'unread', owner: r.owner, repo: r.repo, path: t.file, branch: r.sha, fullContent: true } } }],
      questions: [{ id: 't', questionType: 'locate', target: t.target }],
    };
    const windows = [];
    let host = 0, calls = 0, refs = new Set(), paths = new Set(), sources = [];
    while (request && calls < 20) {
      calls += 1;
      const out = await raw('clasify', { queries: [request] });
      host += out.bytes;
      const q = out.sc?.queries?.[0];
      for (const res of q?.resources ?? []) for (const page of res.pages ?? []) {
        if (page.source?.ref) refs.add(page.source.ref);
        if (page.source?.path) { paths.add(page.source.path); sources.push(page.source); }
      }
      // Compact best rows: `{lines:[start,end], exists, p}`, ranked server-side.
      for (const b of q?.best?.t ?? []) windows.push({ startLine: b.lines[0], endLine: b.lines[1], exists: b.exists, probability: b.p });
      const more = q?.next?.clasify;
      request = more?.query ?? more;
    }
    windows.sort((a, b) => b.exists - a.exists || b.probability - a.probability);
    const top = windows[0];
    const strict = !!top && lines.slice(top.startLine - 1, top.endLine).some(l => t.re.test(l));
    const verify = top ? await call('ghGetFileContent', { owner: r.owner, repo: r.repo, branch: r.sha, path: t.file, startLine: top.startLine, endLine: top.endLine }) : null;
    host += verify?.bytes ?? 0;
    const verified = !!verify && sourceView(fileRow(verify)).text.split('\n').some(l => t.re.test(l));
    rows.push({ file: `${r.repo}/${path.basename(t.file)}`, KB: Math.round(bytes / 1024), calls, window: top ? `${top.startLine}-${top.endLine}` : '-', exists: top?.exists, strict, verified, hostKB: (host / 1024).toFixed(1), saving: `${Math.round(100 - 100 * host / bytes)}%` });
    check(`clasify GitHub ${r.repo}/${path.basename(t.file)}: rank-1 window shows the declaration, verified by a remote read`, strict && verified, `top=${JSON.stringify(top)}`);
    check(`clasify GitHub ${r.repo}/${path.basename(t.file)}: receipts carry owner/repo/path and the pinned ref`, sources.length > 0 && sources.every(s => s.path === `${r.owner}/${r.repo}/${t.file}` && s.ref === r.sha), `paths=${[...paths]} refs=${[...refs].map(x => x.slice(0, 8))}`);
  }
  console.table(rows);
  // Absent target on a remote file stays low everywhere.
  const r = pinned.rust;
  const absent = await raw('clasify', { queries: [{ mainGoal: 'Absent target stays low', reasoning: 'absent', resources: [{ context: { tool: 'ghGetFileContent', query: { reasoning: 'x', owner: r.owner, repo: r.repo, path: 'tokio/src/sync/oneshot.rs', branch: r.sha, fullContent: true } } }], questions: [{ id: 'a', questionType: 'locate', target: 'The function that parses a YAML configuration file into nested dictionaries.' }] }] });
  // Compact pages answer a locate question with the bare exists value.
  const ex = (absent.sc?.queries?.[0]?.resources ?? []).flatMap(res => [res, ...(res.pages ?? [])]).map(p => p.answers?.a).filter(v => typeof v === 'number');
  check('clasify GitHub absent target: every page exists < 0.5', ex.length > 0 && Math.max(...ex) < 0.5, `max=${Math.max(...ex)}`);
  // Scout over a code search: each page is one returned file with its own source.path.
  const scout = await raw('clasify', { queries: [{ mainGoal: 'Rank code-search files', reasoning: 'scout', resources: [{ context: { tool: 'ghSearchCode', query: { reasoning: 'x', owner: r.owner, repo: r.repo, keywords: ['try_recv'], pageSize: 5 } } }], questions: [{ id: 's', type: 'noul', instructions: 'Does this file define the public try_recv method of an mpsc receiver (not a test)?' }] }] });
  // Compact scout pages: the bare P(yes) per file, with the file's path.
  const spages = collect(scout.sc, o => typeof o.answers?.s === 'number' && (o.path ?? o.source?.path)).map(p => ({ path: p.path ?? p.source.path, p: p.answers.s }));
  const topPage = spages.sort((a, b) => b.p - a.p)[0];
  check('clasify GitHub scout: an mpsc receiver file ranks first', !!topPage && /mpsc\/(bounded|unbounded|chan)\.rs$/.test(topPage.path), JSON.stringify(spages.slice(0, 4)));
}

console.log('data compared:', stats, 'rate-limit waits:', rateLimitWaits);
const result = summary();
writeResults('github', { ...result, stats });
client.close();
process.exitCode = result.failed.length ? 1 : 0;

function firstDiff(a, b) {
  if (a === b) return 'identical';
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  const line = a.slice(0, i).split('\n').length;
  return `first diff at char ${i} (line ${line}): tool=${JSON.stringify(a.slice(i, i + 40))} git=${JSON.stringify(b.slice(i, i + 40))} (len ${a.length} vs ${b.length})`;
}
