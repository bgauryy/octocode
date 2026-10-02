// Real-world failures mined from recorded agent sessions (orangu repo/learn
// over the newest sessions), replayed on both surfaces. Each check names the
// input shape agents actually sent and what the tool must now do with it:
// accept it, or reject it with the exact fix.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import fs from 'node:fs';
import { ROOT, checks, collect, rowData, startServer, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('usage-regressions');
const client = await startServer();
const { call, raw } = client;
const brief = { goal: 'octocode-local-testing usage regression', reasoning: 'Replay a recorded agent input shape.' };
const RUNTIME = 'packages/octocode-native/crates/runtime/src';
const LARGE = `${RUNTIME}/contracts/validate.rs`;

/** The CLI as agents call it through a shell: exit code plus parsed stdout. */
function cli(tool, input) {
  const run = spawnSync(process.execPath, [path.join(ROOT, 'packages/octocode/out/octocode.js'), tool, JSON.stringify(input)], {
    cwd: ROOT, encoding: 'utf8', timeout: 120_000, maxBuffer: 64 * 1024 * 1024,
  });
  let json;
  try { json = JSON.parse(run.stdout); } catch { json = undefined; }
  return { exit: run.status, text: `${run.stdout}${run.stderr}`, json, bytes: run.stdout.length };
}
const details = r => (r.json?.details ?? []).join(' | ');
const rowError = r => r.json?.results?.[0]?.data ?? {};

// Batches over the row limit: agents resent the identical 7-row call.
{
  const rows = Array.from({ length: 7 }, () => ({ ...brief, path: 'package.json', startLine: 1, endLine: 3 }));
  const mcp = await raw('localFetch', { queries: rows });
  check('U1 MCP: a 7-row localFetch batch says to split into 2 calls', mcp.isError && /split the batch into 2 calls/.test(mcp.text), mcp.text.slice(0, 200));
  const shell = cli('localFetch', { queries: rows });
  check('U1 CLI: a 7-row localFetch batch exits 2 and says to split into 2 calls', shell.exit === 2 && /split the batch into 2 calls/.test(details(shell)), details(shell));
}

// goal/reasoning beside queries instead of inside each row.
{
  const mcp = await raw('localSearch', { goal: 'g', queries: [{ ...brief, path: RUNTIME, searchText: 'fn validate' }] });
  check('U2 MCP: a top-level goal is moved into each row', mcp.isError && /Move 'goal' into each queries\[\] row/.test(mcp.text), mcp.text.slice(0, 200));
  const shell = cli('localSearch', { goal: 'g', queries: [{ ...brief, path: RUNTIME, searchText: 'fn validate' }] });
  check('U2 CLI: a top-level goal is moved into each row', shell.exit === 2 && /Move 'goal' into each queries\[\] row/.test(details(shell)), details(shell));
}

// Field names agents guessed; the suggestion must be the accepted field.
for (const [label, tool, query, want, never] of [
  ['U3 structureSearch depth', 'structureSearch', { path: 'packages', depth: 2 }, "did you mean 'maxDepth'?", "did you mean 'debug'?"],
  ['U4 localFetch lineStart/lineEnd', 'localFetch', { path: 'package.json', lineStart: 1, lineEnd: 3 }, "did you mean 'startLine'?", null],
  ['U5 localSearch filePattern', 'localSearch', { path: RUNTIME, searchText: 'fn', filePattern: '*.rs' }, "did you mean 'include'?", null],
  ['U6 localSearch isRegex', 'localSearch', { path: RUNTIME, searchText: 'fn', isRegex: true }, "did you mean 'regex'?", null],
  ['U7 localSearch includeHidden', 'localSearch', { path: RUNTIME, searchText: 'fn', includeHidden: true }, "did you mean 'hidden'?", "did you mean 'include'?"],
]) {
  const shell = cli(tool, { queries: [{ ...brief, ...query }] });
  check(`${label}: CLI suggests the accepted field`, shell.exit === 2 && details(shell).includes(want) && (!never || !details(shell).includes(never)), details(shell));
}

// A row sent to the wrong tool names the tool that owns it.
{
  const depth = cli('structureSearch', { queries: [{ ...brief, path: 'packages', depth: 2 }] });
  check('U8 CLI: a single misspelled field is not rerouted to another tool', !/send queries\[0\] to/.test(details(depth)), details(depth));
  const fetch = cli('localFetch', { queries: [{ ...brief, path: 'package.json', searchText: 'name', pageSize: 3 }] });
  check('U8 CLI: localFetch with searchText/pageSize names localSearch', fetch.exit === 2 && /are localSearch fields/.test(details(fetch)), details(fetch));
  const ast = cli('astSearch', { queries: [{ ...brief, path: 'packages', operation: 'files', pageSize: 5 }] });
  check('U9 CLI: astSearch operation "files" names structureSearch', ast.exit === 2 && /is a structureSearch operation/.test(details(ast)), details(ast));
}

// Booleans for on/off enums stay rejected, but the error names the value.
{
  const shell = cli('localSearch', { queries: [{ ...brief, path: RUNTIME, searchText: 'fn validate', regex: true }] });
  check('U10 CLI: regex:true names regex:"rust"', shell.exit === 2 && /use \\?"rust\\?"/.test(details(shell)), details(shell));
}

// Unclosed regex groups: the repair must keep the search's meaning and run.
{
  const grouped = await call('localSearch', { path: RUNTIME, searchText: 'close_unclosed_(group|fn', regex: 'rust' });
  const repair = rowData(grouped)?.next?.repair;
  check('U11: an unclosed group of bare alternatives is closed by next.repair', repair?.query?.searchText === 'close_unclosed_(group|fn)', JSON.stringify(repair?.query?.searchText));
  if (repair) {
    const followed = await raw(repair.tool, { queries: [repair.query] });
    const lines = collect(followed.sc, o => typeof o.value === 'string').map(o => o.value);
    check('U11: following next.repair stays on the grouped term (no bare `fn` hits)', !followed.isError && lines.length > 0 && lines.every(l => l.includes('close_unclosed_group')), `${lines.length} lines; ${lines.find(l => !l.includes('close_unclosed_group'))?.slice(0, 80) ?? ''}`);
  }
  const call_syntax = await call('localSearch', { path: RUNTIME, searchText: 'repair_alternation|close_unclosed_group(', regex: 'rust' });
  const escaped = rowData(call_syntax)?.next?.repair;
  check('U12: call syntax keeps the per-alternative escape', escaped?.query?.searchText === 'repair_alternation|close_unclosed_group\\(', JSON.stringify(escaped?.query?.searchText));
  if (escaped) {
    const followed = await raw(escaped.tool, { queries: [escaped.query] });
    check('U12: following the escaped repair finds the call site', !followed.isError && /close_unclosed_group\(/.test(followed.text), followed.text.slice(0, 160));
  }
}

// Shapes accepted losslessly (bare string for a list field).
{
  const e = await call('localSearch', { path: RUNTIME, searchText: 'fn validate', include: '*.rs', pageSize: 3 });
  check('U13 MCP: include as a bare string is accepted', !e.isError && !e.rowErrors, e.text.slice(0, 160));
}

// Large reads are bounded with an exact continuation, on both surfaces.
for (const [label, query] of [['default', {}], ['fullContent', { fullContent: true }]]) {
  const mcp = await call('localFetch', { path: LARGE, ...query });
  const data = rowData(mcp);
  check(`U14 MCP: localFetch ${label} of a 1,600+ line file stays under 25 KB with next.continue`, mcp.bytes < 25_000 && !!data?.next?.continue && data?.isPartial === true, `${mcp.bytes}B`);
  const shell = cli('localFetch', { queries: [{ ...brief, path: LARGE, ...query }] });
  check(`U14 CLI: localFetch ${label} exits 6 under 25 KB`, shell.exit === 6 && shell.bytes < 25_000, `${shell.exit} ${shell.bytes}B`);
}

// Paths outside the roots: the hint names the trusted place to widen them.
{
  const outside = path.dirname(ROOT);
  const shell = cli('localSearch', { queries: [{ ...brief, path: outside, searchText: 'x' }] });
  const data = rowError(shell);
  check('U15 CLI: pathOutsideAllowedRoots names ALLOWED_PATHS and the home .env', data.errorCode === 'pathOutsideAllowedRoots' && (data.hints ?? []).some(h => h.includes('~/.octocode/.env')), JSON.stringify(data.hints));
  const fetch = await call('localFetch', { path: path.join(outside, 'x.txt') });
  check('U15 MCP: localFetch outside the roots carries the same hint', /~\/\.octocode\/\.env/.test(fetch.text), fetch.text.slice(0, 200));
}

// The retired ghSearch command points at the split tools.
{
  const shell = cli('ghSearch', { queries: [{ ...brief, owner: 'o', repo: 'r', keywords: ['k'] }] });
  check('U16 CLI: legacy ghSearch is rejected with the split tools named', shell.exit === 2 && /ghSearchCode/.test(shell.text) && /ghSearchRepo/.test(shell.text), shell.text.slice(0, 160));
}

// lspSearch on Rust: the first call starts the server, the next one is warm.
{
  const uri = path.join(ROOT, LARGE);
  const lineHint = fs.readFileSync(uri, 'utf8').split('\n').findIndex(l => l.startsWith('fn suggest_field')) + 1;
  const first = await call('lspSearch', { uri, symbolName: 'suggest_field', lineHint, operation: 'references' });
  check('U17 MCP: cold lspSearch references answers without lsp.timeout', !first.isError && !/lsp\.timeout/.test(first.text) && first.ms < 90_000, `${first.ms}ms`);
  const warm = await call('lspSearch', { uri, symbolName: 'suggest_field', lineHint, operation: 'references' });
  check('U17 MCP: a warm lspSearch repeat answers within 15 s', !warm.isError && warm.ms < 15_000, `${warm.ms}ms`);
}

const result = summary();
writeResults('usage-regressions', { ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
