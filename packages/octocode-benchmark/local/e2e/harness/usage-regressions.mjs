// Real-world failures mined from recorded agent sessions (orangu repo/learn
// over the newest sessions), replayed on both surfaces. Each check names the
// input shape agents actually sent and what the tool must now do with it:
// accept it, or reject it with the exact fix.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import fs from 'node:fs';
import { findHint, lspLocations, ROOT, checks, cli as runCli, collect, expandShared, rowData, startServer, structureFiles, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('usage-regressions');
const client = await startServer();
const { call, raw } = client;
const RUNTIME = 'packages/octocode-native/crates/runtime/src';
/** Every Rust source under RUNTIME, workspace-relative. */
const runtimeSources = fs.readdirSync(path.join(ROOT, RUNTIME), { recursive: true })
  .filter(file => file.endsWith('.rs'))
  .map(file => `${RUNTIME}/${file}`);
/** The largest runtime source: chosen by size, so module splits never shrink it under the page window. */
const LARGE = runtimeSources
  .map(file => [file, fs.statSync(path.join(ROOT, file)).size])
  .toSorted((a, b) => b[1] - a[1])[0][0];
/** The runtime source declaring `fn suggest_field`, wherever a split moves it. */
const SUGGEST_FIELD = /^\s*(?:pub(?:\([^)]*\))?\s+)?fn suggest_field\b/;
const SUGGEST = runtimeSources.find(file => fs.readFileSync(path.join(ROOT, file), 'utf8').split('\n').some(line => SUGGEST_FIELD.test(line)));

/** The CLI as agents call it through a shell: exit code plus parsed stdout. */
function cli(tool, input, env = {}) {
  const run = runCli(tool, input, { env, timeout: 120_000 });
  let json;
  try { json = JSON.parse(run.stdout); } catch { json = undefined; }
  return { exit: run.status, text: `${run.stdout}${run.stderr}`, json, bytes: run.stdout.length };
}
// The CLI error payload: the first issue is `error`, the rest `details`.
const details = r => [r.json?.error, ...(r.json?.details ?? [])].filter(Boolean).join(' | ');
const rowError = r => r.json?.results?.[0]?.data ?? {};

// Batches over the row limit: agents resent the identical 7-row call.
{
  const rows = Array.from({ length: 7 }, () => ({ path: 'package.json', ranges: ['1-3'] }));
  const mcp = await raw('localFetch', { queries: rows });
  check('U1 MCP: a 7-row localFetch batch says to split into 2 calls', mcp.isError && /split the batch into 2 calls/.test(mcp.text), mcp.text.slice(0, 200));
  const shell = cli('localFetch', { queries: rows });
  check('U1 CLI: a 7-row localFetch batch exits 2 and says to split into 2 calls', shell.exit === 2 && /split the batch into 2 calls/.test(details(shell)), details(shell));
}

// mainGoal/reasoning beside queries instead of inside each row.
{
  const mcp = await raw('localSearch', { mainGoal: 'g', queries: [{ path: RUNTIME, matchString: 'fn validate' }] });
  check('U2 MCP: a top-level mainGoal is moved into each row', mcp.isError && /Move 'mainGoal' into each queries\[\] row/.test(mcp.text), mcp.text.slice(0, 200));
  const shell = cli('localSearch', { mainGoal: 'g', queries: [{ path: RUNTIME, matchString: 'fn validate' }] });
  check('U2 CLI: a top-level mainGoal is moved into each row', shell.exit === 2 && /Move 'mainGoal' into each queries\[\] row/.test(details(shell)), details(shell));
}

// Field names agents guessed: each is rejected by name, and a suggestion
// (computed from the live schema by spelling) is never an unrelated field.
for (const [label, tool, query, field, want, never] of [
  ['U3 structureSearch depth', 'structureSearch', { path: 'packages', depth: 2 }, 'depth', null, "did you mean 'debug'?"],
  ['U4 localFetch lineStart/lineEnd', 'localFetch', { path: 'package.json', lineStart: 1, lineEnd: 3 }, 'lineStart', null, null],
  ['U5 localSearch filePattern', 'localSearch', { path: RUNTIME, matchString: 'fn', filePattern: '*.rs' }, 'filePattern', null, null],
  ['U6 localSearch isRegex', 'localSearch', { path: RUNTIME, matchString: 'fn', isRegex: true }, 'isRegex', "did you mean 'regex'?", null],
]) {
  const shell = cli(tool, { queries: [query] });
  check(`${label}: CLI rejects the guessed field by name`, shell.exit === 2 && new RegExp(`\\b${field}\\b`).test(details(shell)) && (!want || details(shell).includes(want)) && (!never || !details(shell).includes(never)), details(shell));
}

// A row sent to the wrong tool names the tool that owns it.
{
  const depth = cli('structureSearch', { queries: [{ path: 'packages', depth: 2 }] });
  check('U8 CLI: a single misspelled field is not rerouted to another tool', !/send queries\[0\] to/.test(details(depth)), details(depth));
  // Routed only when localSearch runs the whole row (it requires matchString).
  const fetch = cli('localFetch', { queries: [{ path: 'package.json', matchString: 'name', resultView: 'files', pageSize: 3 }] });
  check('U8 CLI: localFetch with resultView/pageSize names localSearch', fetch.exit === 2 && /are localSearch\b/.test(details(fetch)), details(fetch));
  const ast = cli('astSearch', { queries: [{ path: 'packages', operation: 'files', pageSize: 5 }] });
  check('U9 CLI: astSearch operation "files" names structureSearch', ast.exit === 2 && /is a structureSearch operation/.test(details(ast)), details(ast));
}

// Booleans for on/off enums stay rejected, but the error names the value.
{
  const shell = cli('localSearch', { queries: [{ path: RUNTIME, matchString: 'fn validate', regex: true }] });
  check('U10 CLI: regex:true names regex:"rust"', shell.exit === 2 && /use \\?"rust\\?"/.test(details(shell)), details(shell));
}

// Unclosed regex groups: the repair must keep the search's meaning and run.
{
  const grouped = await call('localSearch', { path: RUNTIME, matchString: 'close_unclosed_(group|fn', regex: 'rust' });
  const repair = findHint(grouped.sc, 'repair');
  check('U11: an unclosed group of bare alternatives is closed by hints.repair', repair?.query?.queries?.[0]?.matchString === 'close_unclosed_(group|fn)', JSON.stringify(repair?.query?.queries?.[0]?.matchString));
  if (repair) {
    const followed = await raw(repair.tool, repair.query);
    const lines = collect(followed.sc, o => typeof o.value === 'string').map(o => o.value);
    check('U11: following hints.repair stays on the grouped term (no bare `fn` hits)', !followed.isError && lines.length > 0 && lines.every(l => l.includes('close_unclosed_group')), `${lines.length} lines; ${lines.find(l => !l.includes('close_unclosed_group'))?.slice(0, 80) ?? ''}`);
  }
  const call_syntax = await call('localSearch', { path: RUNTIME, matchString: 'repair_alternation|close_unclosed_group(', regex: 'rust' });
  const escaped = findHint(call_syntax.sc, 'repair');
  check('U12: call syntax keeps the per-alternative escape', escaped?.query?.queries?.[0]?.matchString === 'repair_alternation|close_unclosed_group\\(', JSON.stringify(escaped?.query?.queries?.[0]?.matchString));
  if (escaped) {
    const followed = await raw(escaped.tool, escaped.query);
    check('U12: following the escaped repair finds the call site', !followed.isError && /close_unclosed_group\(/.test(followed.text), followed.text.slice(0, 160));
  }
}

// Strict admission: a bare string for a list field is rejected with the
// exact wrap repair, and the wrapped value then runs.
{
  const e = await call('localSearch', { path: RUNTIME, matchString: 'fn validate', include: '*.rs', pageSize: 3 });
  check('U13 MCP: include as a bare string is rejected with the wrap repair', e.isError && e.sc?.errorCode === 'invalidInput' && e.sc?.error === 'queries.0.include: Expected array; wrap the value: ["*.rs"]', e.text.slice(0, 160));
  const wrapped = await call('localSearch', { path: RUNTIME, matchString: 'fn validate', include: ['*.rs'], pageSize: 3 });
  check('U13 MCP: the wrapped include runs', !wrapped.isError && !wrapped.rowErrors, wrapped.text.slice(0, 160));
}

// Large reads page inside the configured window with an exact continuation,
// and the walk returns every line once; fullContent pages too, never one dump.
// The window is the runtime's constant automatic page
// (RuntimeLimits::DEFAULT.auto_page_chars); a workspace setting cannot move it.
{
  const limits = fs.readFileSync(path.join(ROOT, 'packages/octocode-native/crates/runtime/src/execution/limits.rs'), 'utf8');
  const WINDOW = Number(/auto_page_chars: ([\d_]+),/.exec(limits)[1].replaceAll('_', ''));
  const lineNumbers = text => [...String(text ?? '').matchAll(/^(\d+)\t/gm)].map(m => Number(m[1]));
  for (const [label, query] of [['default', {}], ['fullContent', { fullContent: true }]]) {
    const mcp = await call('localFetch', { path: LARGE, ...query });
    const data = rowData(mcp);
    check(`U14 MCP: localFetch ${label} of a file over the ${WINDOW}-char window pages within it with next.continueWindow`, mcp.bytes <= WINDOW && !!data?.next?.continueWindow && data?.pagination?.hasMore === true, `${mcp.bytes}B`);
    if (label === 'fullContent') {
      const seen = [...lineNumbers(data?.content)];
      let next = data?.next?.continueWindow;
      let pages = 1;
      let maxBytes = mcp.bytes;
      while (next && pages < 20) {
        const page = await call(next.tool, next.query.queries[0]);
        const row = rowData(page);
        seen.push(...lineNumbers(row?.content));
        maxBytes = Math.max(maxBytes, page.bytes);
        next = row?.next?.continueWindow;
        pages += 1;
      }
      const total = data?.totalLines;
      const exact = seen.length === total && new Set(seen).size === total && seen.every((n, i) => n === i + 1);
      check('U14 MCP: the fullContent walk returns every line once, each page within the window', !next && pages > 1 && exact && maxBytes <= WINDOW, `pages=${pages} lines=${seen.length}/${total} max=${maxBytes}B`);
    }
    // The CLI pages inside the same constant window.
    const shell = cli('localFetch', { queries: [{ path: LARGE, ...query }] });
    const shellRow = shell.json?.results?.[0]?.data;
    check(`U14 CLI: localFetch ${label} at the ${WINDOW}-char window exits 0 under 25 KB with next.continueWindow`, shell.exit === 0 && shell.bytes < 25_000 && !!shellRow?.next?.continueWindow, `${shell.exit} ${shell.bytes}B`);
  }
}

// Paths outside the roots: the hint names the trusted place to widen them.
{
  const outside = path.dirname(ROOT);
  const shell = cli('localSearch', { queries: [{ path: outside, matchString: 'x' }] });
  const data = rowError(shell);
  check('U15 CLI: outsideAllowedRoots names ALLOWED_PATHS and the home .env', data.errorCode === 'outsideAllowedRoots' && (data.hints ?? []).some(h => h.includes('~/.octocode/.env')), JSON.stringify(data.hints));
  const fetch = await call('localFetch', { path: path.join(outside, 'x.txt') });
  check('U15 MCP: localFetch outside the roots carries the same hint', /~\/\.octocode\/\.env/.test(fetch.text), fetch.text.slice(0, 200));
}

// The retired ghSearch command points at the split tools.
{
  const shell = cli('ghSearch', { queries: [{ owner: 'o', repo: 'r', keywords: ['k'] }] });
  check('U16 CLI: legacy ghSearch is rejected with the split tools named', shell.exit === 2 && /ghSearchCode/.test(shell.text) && /ghSearchRepo/.test(shell.text), shell.text.slice(0, 160));
}

// lspSearch on Rust: the first call starts the server, the next one is warm.
{
  const file = path.join(ROOT, SUGGEST);
  const line = fs.readFileSync(file, 'utf8').split('\n').findIndex(l => SUGGEST_FIELD.test(l)) + 1;
  const first = await call('lspSearch', { path: file, symbolName: 'suggest_field', line, operation: 'references' });
  check('U17 MCP: cold lspSearch references answers without a timeout', !first.isError && !/errorCode\W+timeout\b/.test(first.text) && lspLocations(first).length > 0 && first.ms < 90_000, JSON.stringify({ ms: first.ms, locations: lspLocations(first).length, isError: first.isError, errorCode: rowData(first)?.errorCode, error: rowData(first)?.error }));
  const warm = await call('lspSearch', { path: file, symbolName: 'suggest_field', line, operation: 'references' });
  check('U17 MCP: a warm lspSearch repeat answers within 15 s', !warm.isError && lspLocations(warm).length > 0 && warm.ms < 15_000, JSON.stringify({ ms: warm.ms, locations: lspLocations(warm).length, isError: warm.isError, errorCode: rowData(warm)?.errorCode, error: rowData(warm)?.error }));
}

// Continuation pages reuse the page-1 scan; an edit that keeps size and
// nanosecond mtime must not resurface old values or an old key-shaped body.
{
  fs.mkdirSync(path.join(ROOT, '.octocode/tmp'), { recursive: true });
  const dir = fs.mkdtempSync(path.join(ROOT, '.octocode/tmp/cache-binding-'));
  // Rewrite with equal-size bytes, then restore the nanosecond mtime.
  const rewrite = (file, content) => spawnSync('python3', ['-c', 'import os,sys\np=sys.argv[1];s=os.stat(p)\nopen(p,"w").write(sys.argv[2])\nos.utime(p,ns=(s.st_atime_ns,s.st_mtime_ns))\nt=os.stat(p)\nprint(s.st_size==t.st_size and s.st_mtime_ns==t.st_mtime_ns)', file, content], { encoding: 'utf8' }).stdout.trim() === 'True';
  const search = (where, matchString) => ({ path: where, matchString, regex: 'literal', sort: 'path', include: ['*.ts'], includeIgnored: true, pageSize: 1 });
  try {
    fs.writeFileSync(path.join(dir, 'a.ts'), 'const needle = "OLD_A";\n');
    const b = path.join(dir, 'b.ts');
    fs.writeFileSync(b, 'const needle = "OLD_B";\n');
    const next = rowData(await call('localSearch', search(dir, 'needle')))?.next?.nextPage;
    const stamped = rewrite(b, 'const needle = "NEW_B";\n');
    const replay = next ? await raw(next.tool, next.query) : { text: 'no nextPage' };
    check('U18 MCP: a same-size, same-mtime edit restarts the continuation instead of serving old text', stamped && !replay.text.includes('OLD_B') && /staleSnapshot/.test(replay.text) && /restart/.test(replay.text), replay.text.slice(0, 200));

    const keydir = path.join(dir, 'key');
    fs.mkdirSync(keydir);
    const body = 'QUJD'.repeat(24);
    fs.writeFileSync(path.join(keydir, 'a.ts'), '// QUJD visible harmless marker\n');
    const keyfile = path.join(keydir, 'b.ts');
    fs.writeFileSync(keyfile, `-----BEGIN PRIVATE KEY-----\n${body}\n-----END PRIVATE KEY-----\n`);
    const keyNext = rowData(await call('localSearch', search(keydir, 'QUJD')))?.next?.nextPage;
    const before = keyNext ? await raw(keyNext.tool, keyNext.query) : { text: 'no nextPage' };
    check('U18 MCP: the stored key-shaped value is redacted on its page', !before.text.includes(body) && /REDACTED/.test(before.text), before.text.slice(0, 200));
    const swapped = rewrite(keyfile, `// clean source replacement\n${'Z'.repeat(body.length)}\n${'// clean trailer'.padEnd('-----END PRIVATE KEY-----'.length)}\n`);
    const after = keyNext ? await raw(keyNext.tool, keyNext.query) : { text: 'no nextPage' };
    check('U18 MCP: after a stamp-preserving swap the old key-shaped body never surfaces', swapped && !after.text.includes(body), after.text.slice(0, 200));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// Default-layout page walks: every streamed page fits one response window,
// so following ONLY each row's next.nextPage reaches every row exactly once.
// A page larger than the window used to split into row parts whose later
// part had no continuation, and the row walk silently skipped its rows.
{
  const env = {};
  const walker = await startServer({ env });
  const rowWalk = async (surface, tool, query, rowsOf, totalOf) => {
    let input = { queries: [query] };
    const out = { seen: [], calls: 0, bytes: 0, splits: 0, total: undefined, exits: [] };
    while (out.calls < 40) {
      out.calls += 1;
      let sc;
      if (surface === 'mcp') {
        const r = await walker.raw(tool, input);
        sc = r.sc; out.bytes += r.bytes;
      } else {
        const r = cli(tool, input, env);
        sc = expandShared(r.json); out.bytes += r.bytes; out.exits.push(r.exit);
      }
      const data = sc?.results?.[0]?.data ?? {};
      if (sc?.responsePagination || sc?.results?.[0]?.rowPart) out.splits += 1;
      out.total ??= totalOf(data);
      out.seen.push(...rowsOf(data));
      const next = data.next?.nextPage;
      if (!next) break;
      input = next.query;
    }
    out.unique = new Set(out.seen).size;
    return out;
  };
  const hits = data => (data.files ?? []).flatMap(f => (f.matches ?? []).map(m => `${f.path}:${m.line}`));
  const files = data => structureFiles(data.files, data.path).map(f => f.path);
  for (const [label, tool, query, rowsOf, totalOf] of [
    ['localSearch .unwrap() over runtime tools', 'localSearch', { path: `${RUNTIME}/tools`, matchString: '.unwrap()' }, hits, d => d.matchedLineCount ?? hits(d).length],
    // About 2.9k .py files: several pages even with directory-grouped rows.
    // The listing total (`pagination.totalItems`) is a stats extra; without
    // it the first page's row count is not the total.
    ['structureSearch **/*.py over the python corpus', 'structureSearch', { operation: 'files', path: 'packages/octocode-benchmark/local/repos/python', include: ['**/*.py'], extras: ['stats'] }, files, d => d.pagination?.totalItems],
  ]) {
    for (const surface of ['mcp', 'cli']) {
      const w = await rowWalk(surface, tool, query, rowsOf, totalOf);
      const stats = `${w.unique}/${w.total} unique, ${w.seen.length} shown, ${w.calls} calls, ${w.bytes}B, ${w.splits} split`;
      check(`U19 ${surface.toUpperCase()}: ${label} spans several pages`, w.calls > 1, stats);
      check(`U19 ${surface.toUpperCase()}: ${label} pages each fit one response (no row parts)`, w.splits === 0, stats);
      check(`U19 ${surface.toUpperCase()}: ${label} row-continuation walk shows every row exactly once`, w.total > 0 && w.unique === w.total && w.seen.length === w.total, stats);
      if (surface === 'cli') check(`U19 CLI: ${label} exits 0 on every page, including those with a next page`, w.exits.length > 0 && w.exits.every(code => code === 0), w.exits.join(','));
    }
  }
  walker.close();
}

const result = summary();
writeResults('usage-regressions', { ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
