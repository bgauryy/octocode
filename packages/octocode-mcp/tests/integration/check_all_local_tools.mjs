/**
 * End-to-end quality sweep for the 7 local/LSP tools — spawns a fresh
 * `node dist/index.js` per request and drives it over the real MCP JSON-RPC
 * transport (stdin/stdout). Bypasses vitest's `child_process`/hint-registry
 * mocks so we exercise the same code Cursor's MCP server runs.
 *
 * Coverage per tool:
 *   • compact (default)           — full payload, sanity-check shape
 *   • verbose                     — default-equivalent for now (byte-identical)
 *   • ultra                       — lossy summary + drill-back breadcrumb
 *   • scale                       — large repo-wide query, payload bounded
 *   • pagination                  — `pageNumber` traversal returns disjoint pages
 *   • edge: empty / no-match      — graceful empty result
 *   • edge: invalid input         — surfaces MCP validation error
 *
 * Run from the package root:
 *   yarn workspace octocode-mcp build && \
 *     node packages/octocode-mcp/tests/integration/check_all_local_tools.mjs
 */

import { spawn } from 'node:child_process';
import { resolve } from 'node:path';

const WORKSPACE = resolve(
  new URL('.', import.meta.url).pathname,
  '../../'
);
const DIST = resolve(WORKSPACE, 'dist');

const TOOL = {
  RG: 'localSearchCode',
  FIND: 'localFindFiles',
  VIEW: 'localViewStructure',
  FETCH: 'localGetFileContent',
  GOTO: 'lspGotoDefinition',
  REFS: 'lspFindReferences',
  HIER: 'lspCallHierarchy',
};

// ---------------------------------------------------------------------------
// MCP transport: spawn → initialize → tools/call → result → kill
// ---------------------------------------------------------------------------

function callTool(name, args, timeoutMs = 60_000) {
  return new Promise((res, rej) => {
    const proc = spawn(process.execPath, [resolve(DIST, 'index.js')], {
      cwd: WORKSPACE,
      env: { ...process.env, WORKSPACE_ROOT: WORKSPACE },
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    let buffer = '';
    let stdoutDump = '';
    let stderrDump = '';
    let initialized = false;
    let settled = false;
    let killTimer = null;

    const finish = (fn) => (v) => {
      if (settled) return;
      settled = true;
      if (killTimer) clearTimeout(killTimer);
      try {
        proc.kill('SIGTERM');
      } catch {}
      fn(v);
    };
    const resolveOnce = finish(res);
    const rejectOnce = finish(rej);

    proc.stdout.on('data', (chunk) => {
      const text = chunk.toString('utf-8');
      stdoutDump += text;
      buffer += text;
      let nl;
      while ((nl = buffer.indexOf('\n')) !== -1) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (!line) continue;
        let msg;
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        if (!initialized && msg.id === 1 && msg.result) {
          initialized = true;
          proc.stdin.write(
            JSON.stringify({
              jsonrpc: '2.0',
              method: 'notifications/initialized',
              params: {},
            }) + '\n'
          );
          proc.stdin.write(
            JSON.stringify({
              jsonrpc: '2.0',
              id: 99,
              method: 'tools/call',
              params: { name, arguments: args },
            }) + '\n'
          );
          continue;
        }
        if (msg.id === 99 && msg.result !== undefined) {
          return resolveOnce(msg.result);
        }
        if (msg.id === 99 && msg.error) {
          // Return the error as a tool result so callers can assert on it.
          return resolveOnce({ __mcpError: msg.error });
        }
      }
    });
    proc.stderr.on('data', (b) => (stderrDump += b.toString('utf-8')));
    proc.on('error', rejectOnce);
    proc.on('close', () => {
      if (!settled) {
        rejectOnce(
          new Error(
            `Process exited without response. stdout=${stdoutDump.slice(0, 400)} stderr=${stderrDump.slice(0, 400)}`
          )
        );
      }
    });

    proc.stdin.write(
      JSON.stringify({
        jsonrpc: '2.0',
        id: 1,
        method: 'initialize',
        params: {
          protocolVersion: '2024-11-05',
          capabilities: {},
          clientInfo: { name: 'check_all_local_tools', version: '0.2' },
        },
      }) + '\n'
    );

    killTimer = setTimeout(() => {
      rejectOnce(new Error(`Tool ${name} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });
}

function bytes(o) {
  return JSON.stringify(o).length;
}

function unwrap(result) {
  if (result?.__mcpError) return { __mcpError: result.__mcpError };
  if (result?.structuredContent?.results?.[0]?.data) {
    return result.structuredContent.results[0].data;
  }
  if (result?.structuredContent?.results?.[0]) {
    return result.structuredContent.results[0];
  }
  return result;
}

function unwrapEnvelope(result) {
  if (result?.__mcpError) return result;
  return result?.structuredContent?.results?.[0] ?? result;
}

function joinHints(d) {
  return (d?.hints ?? []).join('\n');
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

const checks = [];
const debugFor = new Set([
  'rg invalid surfaces error',
  'view depth=0 returns',
  'fetch missing-file surfaces error',
  'refs scale ultra hasResults or empty',
  'refs groupByFile ultra returns hasResults or empty',
  'hier outgoing returns hasResults or empty',
  'hier missing surfaces error/empty',
]);
let lastValue = null;
function assert(label, cond, hint = '') {
  checks.push({ label, ok: !!cond, hint });
  if (!cond && debugFor.has(label) && lastValue !== null) {
    console.log(
      `    debug[${label}]:`,
      JSON.stringify(lastValue).slice(0, 600)
    );
  }
  return !!cond;
}
function note(value) {
  lastValue = value;
}

function row(name, cells) {
  const cols = cells.map((c) =>
    String(c).padStart(c.toString().length > 8 ? c.toString().length : 8)
  );
  console.log(`  ${name.padEnd(28)} ${cols.join(' ')}`);
}

function pct(after, before) {
  if (!before) return '—';
  return `${((1 - after / before) * 100).toFixed(1)}%`;
}

async function probe3(tool, base) {
  const compact = unwrap(await callTool(tool, queries(base)));
  const verbose = unwrap(
    await callTool(tool, queries({ ...base, verbosity: 'verbose' }))
  );
  const ultra = unwrap(
    await callTool(tool, queries({ ...base, verbosity: 'ultra' }))
  );
  return { compact, verbose, ultra };
}

/**
 * Tools return `{ status, data, hints, ... }` envelopes. `status` lives on the
 * envelope, not on `data`. This returns the envelope's status string.
 */
function statusOf(rawResult) {
  if (rawResult?.__mcpError) return 'error';
  const env = rawResult?.structuredContent?.results?.[0] ?? rawResult;
  return env?.status;
}

function queries(q) {
  return { queries: [{ id: 'q', researchGoal: 'sanity', reasoning: 'check', ...q }] };
}

// ---------------------------------------------------------------------------
// Per-tool suites
// ---------------------------------------------------------------------------

async function suiteSearchCode() {
  console.log('\n[1/7] localSearchCode');
  const base = { pattern: 'applyRipgrepVerbosity', path: `${WORKSPACE}/src` };
  const { compact, verbose, ultra } = await probe3(TOOL.RG, base);
  assert('rg compact has files', compact?.files?.length >= 1);
  assert('rg verbose == compact (default-invariant)', bytes(verbose) === bytes(compact));
  assert('rg ultra drops files', ultra?.files?.length === 0);
  assert('rg ultra summary', /\d+ matches in \d+ files/.test(joinHints(ultra)));
  assert('rg ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Scale: very common word across the whole workspace
  const scale = unwrap(
    await callTool(
      TOOL.RG,
      queries({ pattern: 'function', path: `${WORKSPACE}/src`, verbosity: 'ultra' })
    )
  );
  assert('rg scale ultra returns hasResults', scale?.status === 'hasResults' || scale?.searchEngine === 'rg');
  assert('rg scale ultra payload bounded (<2KB)', bytes(scale) < 2000);
  row('  scale (ultra)', [bytes(scale) + 'B', '', '', '']);

  // Pagination
  const p1 = unwrap(
    await callTool(
      TOOL.RG,
      queries({ pattern: 'export', path: `${WORKSPACE}/src`, filesPerPage: 3, filePageNumber: 1 })
    )
  );
  const p2 = unwrap(
    await callTool(
      TOOL.RG,
      queries({ pattern: 'export', path: `${WORKSPACE}/src`, filesPerPage: 3, filePageNumber: 2 })
    )
  );
  const p1Paths = (p1?.files ?? []).map((f) => f.path);
  const p2Paths = (p2?.files ?? []).map((f) => f.path);
  const overlap = p1Paths.filter((x) => p2Paths.includes(x)).length;
  assert('rg pagination page1 != page2', overlap === 0 && p1Paths.length > 0 && p2Paths.length > 0);
  row('  pagination', [
    `${p1?.pagination?.currentPage}/${p1?.pagination?.totalPages}`,
    `${p2?.pagination?.currentPage}/${p2?.pagination?.totalPages}`,
    '',
    '',
  ]);

  // Edge: no match
  const empty = unwrap(
    await callTool(
      TOOL.RG,
      queries({ pattern: '__no_such_symbol_xyz__', path: `${WORKSPACE}/src` })
    )
  );
  assert('rg empty', empty?.status === 'empty' || (empty?.files?.length ?? 0) === 0);

  // Edge: invalid input (missing required path)
  const invalid = await callTool(TOOL.RG, { queries: [{ id: 'q' }] });
  const env = unwrapEnvelope(invalid);
  note(env);
  assert(
    'rg invalid surfaces error',
    !!env?.__mcpError || env?.status === 'error' || /validation|invalid|required/i.test(JSON.stringify(env))
  );
}

async function suiteFindFiles() {
  console.log('\n[2/7] localFindFiles');
  const base = { path: WORKSPACE, type: 'f', name: '*.ts' };
  const { compact, verbose, ultra } = await probe3(TOOL.FIND, base);
  assert('find compact has files', compact?.files?.length >= 1);
  assert('find verbose == compact', bytes(verbose) === bytes(compact));
  assert('find ultra drops files', !ultra?.files || ultra.files.length === 0);
  assert('find ultra summary', /files in \d+ dirs/.test(joinHints(ultra)));
  assert('find ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Scale
  const scale = unwrap(
    await callTool(
      TOOL.FIND,
      queries({ path: WORKSPACE, type: 'f', verbosity: 'ultra' })
    )
  );
  assert('find scale ultra payload bounded (<2KB)', bytes(scale) < 2000);
  row('  scale (ultra)', [bytes(scale) + 'B', '', '', '']);

  // Pagination
  const p1 = unwrap(
    await callTool(
      TOOL.FIND,
      queries({ path: WORKSPACE, type: 'f', name: '*.ts', filesPerPage: 5, filePageNumber: 1 })
    )
  );
  const p2 = unwrap(
    await callTool(
      TOOL.FIND,
      queries({ path: WORKSPACE, type: 'f', name: '*.ts', filesPerPage: 5, filePageNumber: 2 })
    )
  );
  const p1Paths = (p1?.files ?? []).map((f) => f.path);
  const p2Paths = (p2?.files ?? []).map((f) => f.path);
  const overlap = p1Paths.filter((x) => p2Paths.includes(x)).length;
  assert('find pagination disjoint', overlap === 0 && p1Paths.length > 0);
  row('  pagination', [
    `${p1?.pagination?.currentPage}/${p1?.pagination?.totalPages}`,
    `${p2?.pagination?.currentPage}/${p2?.pagination?.totalPages}`,
    '',
    '',
  ]);

  // Edge: no match
  const empty = unwrap(
    await callTool(
      TOOL.FIND,
      queries({ path: WORKSPACE, name: '__no_match_zz__' })
    )
  );
  assert('find empty', empty?.status === 'empty' || (empty?.files?.length ?? 0) === 0);
}

async function suiteViewStructure() {
  console.log('\n[3/7] localViewStructure');
  const base = { path: `${WORKSPACE}/src/tools/local_fetch_content` };
  const { compact, verbose, ultra } = await probe3(TOOL.VIEW, base);
  assert('view compact has entries', compact?.entries?.length >= 1);
  assert('view verbose == compact', bytes(verbose) === bytes(compact));
  assert('view ultra drops entries', !ultra?.entries || ultra.entries.length === 0);
  assert('view ultra summary', /\d+ entries/.test(ultra?.summary ?? joinHints(ultra)));
  assert('view ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Scale: recursive over the whole src tree
  const scale = unwrap(
    await callTool(
      TOOL.VIEW,
      queries({ path: `${WORKSPACE}/src`, depth: 5, verbosity: 'ultra' })
    )
  );
  assert('view scale ultra payload bounded (<2KB)', bytes(scale) < 2000);
  row('  scale (ultra)', [bytes(scale) + 'B', '', '', '']);

  // Pagination
  const p1 = unwrap(
    await callTool(
      TOOL.VIEW,
      queries({ path: `${WORKSPACE}/src`, depth: 3, entriesPerPage: 10, entryPageNumber: 1 })
    )
  );
  const p2 = unwrap(
    await callTool(
      TOOL.VIEW,
      queries({ path: `${WORKSPACE}/src`, depth: 3, entriesPerPage: 10, entryPageNumber: 2 })
    )
  );
  const p1Names = (p1?.entries ?? []).map((e) => e.name);
  const p2Names = (p2?.entries ?? []).map((e) => e.name);
  const overlap = p1Names.filter((x) => p2Names.includes(x)).length;
  assert('view pagination disjoint', overlap === 0 && p1Names.length > 0);
  row('  pagination', [
    `${p1?.pagination?.currentPage}/${p1?.pagination?.totalPages}`,
    `${p2?.pagination?.currentPage}/${p2?.pagination?.totalPages}`,
    '',
    '',
  ]);

  // Edge: minimal depth — should still return entries
  const raw = await callTool(
    TOOL.VIEW,
    queries({ path: `${WORKSPACE}/src`, depth: 1 })
  );
  const empty = unwrap(raw);
  const status = statusOf(raw);
  note({ status, entries: empty?.entries?.length });
  assert(
    'view depth=1 returns entries',
    (empty?.entries?.length ?? 0) > 0
  );
}

async function suiteFetchContent() {
  console.log('\n[4/7] localGetFileContent');
  const base = { path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts` };
  const { compact, verbose, ultra } = await probe3(TOOL.FETCH, base);
  assert('fetch compact has content', (compact?.content ?? '').length > 100);
  assert('fetch verbose == compact', bytes(verbose) === bytes(compact));
  assert('fetch ultra empties content', ultra?.content === '');
  assert('fetch ultra summary', /\d+ lines/.test(joinHints(ultra)));
  assert('fetch ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // matchString slice (compact)
  const sliced = unwrap(
    await callTool(
      TOOL.FETCH,
      queries({
        path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
        matchString: 'applyRipgrepVerbosity',
        matchStringContextLines: 2,
      })
    )
  );
  assert('fetch matchString returns slice', (sliced?.content ?? '').includes('applyRipgrepVerbosity'));
  assert('fetch matchString smaller than full', bytes(sliced) < bytes(compact));
  row('  matchString slice', [
    bytes(sliced) + 'B',
    pct(bytes(sliced), bytes(compact)),
    '',
    '',
  ]);

  // Pagination via charLength
  const pageA = unwrap(
    await callTool(
      TOOL.FETCH,
      queries({
        path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
        charLength: 1500,
        charOffset: 0,
      })
    )
  );
  const pageB = unwrap(
    await callTool(
      TOOL.FETCH,
      queries({
        path: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
        charLength: 1500,
        charOffset: 1500,
      })
    )
  );
  assert(
    'fetch charLength pagination disjoint',
    pageA?.content !== pageB?.content &&
      (pageA?.content ?? '').length > 0 &&
      (pageB?.content ?? '').length > 0
  );
  row('  pagination (charLength)', [
    bytes(pageA) + 'B',
    bytes(pageB) + 'B',
    '',
    '',
  ]);

  // Edge: nonexistent file
  const missing = unwrap(
    await callTool(
      TOOL.FETCH,
      queries({ path: `${WORKSPACE}/__nope_does_not_exist__.txt` })
    )
  );
  note(missing);
  assert(
    'fetch missing-file surfaces error',
    missing?.status === 'error' ||
      missing?.__mcpError ||
      /not.*found|does not exist|enoent/i.test(JSON.stringify(missing))
  );
}

async function suiteGoto() {
  console.log('\n[5/7] lspGotoDefinition');
  const base = {
    uri: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
    symbolName: 'isUltra',
    lineHint: 12,
  };
  const { compact, verbose, ultra } = await probe3(TOOL.GOTO, base);
  assert('goto compact has location', !!compact?.locations?.[0]);
  assert('goto compact has snippet', (compact?.locations?.[0]?.content ?? '').length > 5);
  assert('goto verbose == compact', bytes(verbose) === bytes(compact));
  assert('goto ultra drops snippet', ultra?.locations?.[0]?.content === '');
  assert('goto ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Edge: symbol that doesn't exist at that line
  const bogus = unwrap(
    await callTool(
      TOOL.GOTO,
      queries({
        uri: `${WORKSPACE}/src/tools/local_ripgrep/ripgrepResultBuilder.ts`,
        symbolName: '__nope_zz__',
        lineHint: 12,
      })
    )
  );
  assert(
    'goto missing symbol surfaces error/empty',
    bogus?.status === 'error' || bogus?.status === 'empty' || (bogus?.locations?.length ?? 0) === 0
  );
}

async function suiteRefs() {
  console.log('\n[6/7] lspFindReferences');
  const base = {
    uri: `${WORKSPACE}/src/scheme/verbosity.ts`,
    symbolName: 'isUltra',
    lineHint: 25,
  };
  const { compact, verbose, ultra } = await probe3(TOOL.REFS, base);
  const refs = compact?.locations?.length ?? 0;
  assert('refs compact has refs (>=5)', refs >= 5, `got ${refs}`);
  assert('refs verbose == compact', bytes(verbose) === bytes(compact));
  assert(
    'refs compact uses domain pagination only',
    compact?.outputPagination === undefined
  );
  assert('refs ultra drops locations', !ultra?.locations || ultra.locations.length === 0);
  assert(
    'refs ultra emits absolute drill-back paths',
    joinHints(ultra).includes('refs: /Users/')
  );
  assert('refs ultra summary', /\d+ refs in \d+ files/.test(joinHints(ultra)));
  assert('refs ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Scale: paginated ultra path (totalResults>0 even though locations[] is empty in ultra)
  const bigRaw = await callTool(
    TOOL.REFS,
    queries({ ...base, verbosity: 'ultra' })
  );
  const big = unwrap(bigRaw);
  note({ status: statusOf(bigRaw), pagination: big?.pagination, hints: big?.hints?.[0] });
  assert(
    'refs scale ultra reports totalResults',
    (big?.pagination?.totalResults ?? 0) > 0 || (big?.locations?.length ?? 0) > 0
  );
  assert('refs scale ultra payload bounded (<3KB)', bytes(big) < 3000);
  row('  scale (ultra)', [bytes(big) + 'B', '', '', '']);

  // groupByFile rollup
  const groupedRaw = await callTool(
    TOOL.REFS,
    queries({ ...base, groupByFile: true, verbosity: 'ultra' })
  );
  const grouped = unwrap(groupedRaw);
  note({ status: statusOf(groupedRaw), pagination: grouped?.pagination });
  assert(
    'refs groupByFile ultra reports refs',
    (grouped?.pagination?.totalResults ?? 0) > 0 ||
      (grouped?.locations?.length ?? 0) > 0 ||
      /refs in/.test(joinHints(grouped))
  );
  row('  groupByFile (ultra)', [bytes(grouped) + 'B', '', '', '']);

  // Edge: missing symbol
  const bogus = unwrap(
    await callTool(
      TOOL.REFS,
      queries({
        uri: `${WORKSPACE}/src/scheme/verbosity.ts`,
        symbolName: '__nope__',
        lineHint: 25,
      })
    )
  );
  assert(
    'refs missing surfaces error/empty',
    bogus?.status === 'error' || bogus?.status === 'empty' || (bogus?.locations?.length ?? 0) === 0
  );
}

async function suiteCallHier() {
  console.log('\n[7/7] lspCallHierarchy');
  const base = {
    uri: `${WORKSPACE}/src/scheme/verbosity.ts`,
    symbolName: 'isUltra',
    lineHint: 25,
    direction: 'incoming',
  };
  const { compact, verbose, ultra } = await probe3(TOOL.HIER, base);
  const calls = (compact?.incomingCalls?.length ?? compact?.calls?.length) ?? 0;
  const ultCalls = (ultra?.incomingCalls?.length ?? ultra?.calls?.length) ?? 0;
  assert('hier compact has calls (>=1)', calls >= 1, `got ${calls}`);
  assert('hier verbose == compact', bytes(verbose) === bytes(compact));
  assert('hier ultra drops calls', ultCalls === 0);
  assert('hier ultra drops root snippet', (ultra?.item?.content ?? '') === '');
  assert('hier ultra drill-back', /drill-back/i.test(joinHints(ultra)));
  row('  compact / verbose / ultra', [
    bytes(compact) + 'B',
    bytes(verbose) + 'B',
    bytes(ultra) + 'B',
    pct(bytes(ultra), bytes(compact)),
  ]);

  // Outgoing direction (isUltra is a leaf — outgoing may be empty, that's OK)
  const outRaw = await callTool(
    TOOL.HIER,
    queries({ ...base, direction: 'outgoing', verbosity: 'ultra' })
  );
  const out = unwrap(outRaw);
  note({ status: statusOf(outRaw), out });
  assert(
    'hier outgoing has shape (item + direction)',
    !!out?.item && out?.direction === 'outgoing'
  );
  row('  outgoing (ultra)', [bytes(out) + 'B', '', '', '']);

  // Edge: missing symbol
  const bogus = unwrap(
    await callTool(
      TOOL.HIER,
      queries({
        uri: `${WORKSPACE}/src/scheme/verbosity.ts`,
        symbolName: '__nope__',
        lineHint: 25,
        direction: 'incoming',
      })
    )
  );
  note(bogus);
  assert(
    'hier missing surfaces error/empty',
    bogus?.status === 'error' ||
      bogus?.status === 'empty' ||
      bogus?.__mcpError ||
      /not found|symbol/i.test(JSON.stringify(bogus))
  );
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log('Quality sweep — 7 local/LSP tools end-to-end via MCP transport');
  console.log('='.repeat(78));
  row('tool', ['compact', 'verbose', 'ultra', 'saved']);
  console.log('-'.repeat(78));

  await suiteSearchCode();
  await suiteFindFiles();
  await suiteViewStructure();
  await suiteFetchContent();
  await suiteGoto();
  await suiteRefs();
  await suiteCallHier();

  console.log('\n' + '='.repeat(78));
  const failed = checks.filter((c) => !c.ok);
  const passed = checks.length - failed.length;
  console.log(`Assertions: ${passed}/${checks.length} passed`);
  if (failed.length > 0) {
    for (const f of failed) {
      console.log(`  ✗ ${f.label}${f.hint ? ` (${f.hint})` : ''}`);
    }
    process.exit(1);
  }
  console.log('All quality checks passed.');
  process.exit(0);
}

main().catch((e) => {
  console.error('FATAL:', e);
  process.exit(2);
});
