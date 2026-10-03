// Regression workflows for every defect fixed in this pass, plus
// continuation hygiene: each `next.*` met along the way must execute.
import fs from 'node:fs';
import path from 'node:path';
import { FIXTURES, ROOT, checks, collect, declarations, lspCallers, nextHints, rowData, startServer, writeResults, lspLocations } from './mcp-client.mjs';

const { check, summary } = checks('workflows');
const client = await startServer();
const { call, raw } = client;
const NATIVE = path.join(ROOT, 'packages/octocode-native');
const LSP = path.join(FIXTURES, 'lsp');
const locations = e => lspLocations(e).map(l => path.resolve(e.sc?.base ?? '/', l.path));
const callers = e => lspCallers(e).map(c => `${c.name}@${path.basename(c.path ?? '')}`);

// F1 — TS/JS importers from the declaration side (inferred project, CommonJS alias).
for (const [variant, ext] of [['cjs-noconfig', 'cjs'], ['cjs-jsconfig', 'cjs'], ['esm-noconfig', 'mjs'], ['esm-tsconfig', 'ts']]) {
  const uri = path.join(LSP, variant, `a.${ext}`);
  const refs = await call('lspSearch', { uri, symbolName: 'stage', lineHint: 1, operation: 'references' });
  check(`F1 ${variant}: references reach the importer b.${ext}`, locations(refs).some(f => f.endsWith(`b.${ext}`)), locations(refs).map(f => path.basename(f)).join(','));
  const up = await call('lspSearch', { uri, symbolName: 'stage', lineHint: 1, operation: 'callers' });
  check(`F1 ${variant}: callers find run`, callers(up).some(c => c.startsWith('run@b.')), callers(up).join(','));
}
{
  const uri = path.join(NATIVE, 'scripts/native-addon-utils.cjs');
  // Anchor on the live declaration line: this is a real repo file that moves.
  const lineHint = fs.readFileSync(uri, 'utf8').split('\n').findIndex(line => line.startsWith('function stageFile(')) + 1;
  const refs = await call('lspSearch', { uri, symbolName: 'stageFile', lineHint, operation: 'references' });
  const files = new Set(locations(refs).map(f => path.basename(f)));
  check('F1 real repo: stageFile references span build-native.cjs + test', files.has('build-native.cjs') && files.has('build-publication.test.cjs'), [...files].join(','));
  const coverage = rowData(refs)?.payload?.coverage;
  check('F1: coverage reports a complete importer scan', coverage?.importerScan === 'complete', JSON.stringify(coverage));
}

// F2 — workspaceSymbol ranks the exact name first.
{
  const ws = await call('lspSearch', { uri: path.join(NATIVE, 'crates/runtime/src/process_status.rs'), operation: 'workspaceSymbol', symbolName: 'is_alive' });
  const first = rowData(ws)?.payload?.items?.[0]?.name;
  check('F2: exact workspace symbol ranks first', first === 'is_alive', first);
}

// F3 — root-only search in a mixed TS+Rust root routes to the other language.
{
  const ws = await call('lspSearch', { workspaceRoot: NATIVE, operation: 'workspaceSymbol', symbolName: 'is_alive' });
  const route = nextHints(ws.sc).find(h => h.path.endsWith('.searchRust'));
  check('F3: mixed root names the searched language and offers next.searchRust', (rowData(ws)?.lsp?.language ?? (/\btypescript language server\b/.test(JSON.stringify(rowData(ws)?.hints ?? [])) ? 'typescript' : undefined)) === 'typescript' && !!route, JSON.stringify(rowData(ws)?.hints));
  if (route) {
    const followed = await raw(route.tool, route.query);
    check('F3: following next.searchRust finds is_alive', (rowData(followed)?.payload?.items ?? []).some(i => i.name === 'is_alive'), followed.text.slice(0, 100));
  }
}

// F7 — pasted ast-grep rule files run as-is; unsupported operators still fail clearly.
{
  const rule = 'id: unsafe-fn\nlanguage: rust\nseverity: warning\nmessage: x\nrule:\n  kind: function_item\n  has:\n    pattern: unsafe { $$$ }\n    stopBy: end\n';
  const e = await call('astSearch', { operation: 'match', path: path.join(NATIVE, 'crates/runtime/src'), langType: 'rust', resultView: 'files', rule });
  check('F7: rule file with metadata keys matches', !e.rowErrors && collect(rowData(e), o => typeof o.path === 'string').length > 0, e.text.slice(0, 100));
  const bad = await call('astSearch', { operation: 'match', path: path.join(NATIVE, 'crates/runtime/src'), langType: 'rust', rule: 'rule:\n  pattern: $A\nconstraints: {}\n' });
  check('F7: unsupported constraints still rejected with the supported field list', bad.rowErrors === 1 && /constraints/.test(bad.text), bad.text.slice(0, 120));
}

// F8 / secrets — redaction on every surface, no matchString oracle, honest hint.
{
  const dir = path.join(FIXTURES, 'secret');
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, 'config.js');
  // Fake fixtures, split so secret scanners never match this source file.
  const AWS = 'AKIA' + 'IOSFODNN7EXAMPLE';
  const GHP = 'ghp_' + '1234567890abcdefghijklmnopqrstuvwxyzAB';
  const PEM_BEGIN = '-----BEGIN RSA ' + 'PRIVATE KEY-----';
  fs.writeFileSync(file, "const awsKey = '" + AWS + "';\nconst token = '" + GHP + "';\n" + PEM_BEGIN + "\nMIIEowIBAAKCAQEAu1SU1LfVLPHCozMxH2Mo4lgOEePzNm0tRgeLezV6ffAt0gun\n" + PEM_BEGIN.replace('BEGIN', 'END') + "\nmodule.exports = { awsKey };\n");
  for (const [label, entry] of [
    ['grep', await call('localSearch', { path: dir, searchText: 'AKIA' })],
    ['grep PEM body', await call('localSearch', { path: dir, searchText: 'MIIEow' })],
    ['fetch', await call('localFetch', { path: file, fullContent: true })],
  ]) check(`secrets: ${label} never returns the secret`, ![AWS, GHP.slice(0, 14), 'MIIEowIBAAKCAQEA'].some(secret => entry.text.includes(secret)), entry.text.slice(0, 80));
  const right = await call('localFetch', { path: file, matchString: 'ghp_1234' });
  const wrong = await call('localFetch', { path: file, matchString: 'ghp_9999' });
  check('F8: right and wrong secret guesses are indistinguishable', JSON.stringify(rowData(right)?.hints) === JSON.stringify(rowData(wrong)?.hints) && rowData(right)?.errorCode === rowData(wrong)?.errorCode);
  check('F8: the miss explains redaction', (rowData(right)?.hints ?? []).some(h => h.includes('REDACTED')), JSON.stringify(rowData(right)?.hints));
  fs.rmSync(dir, { recursive: true, force: true });
}

// F9 — directory symbols with langType returns an exact, executable repair.
{
  const bad = await call('astSearch', { operation: 'symbols', path: path.join(NATIVE, 'scripts'), langType: 'JavaScript' });
  const repair = nextHints(bad.sc).find(h => h.path.endsWith('.repair'));
  check('F9: repair continuation drops langType', !!repair && !('langType' in repair.query), JSON.stringify(rowData(bad)?.hints));
  if (repair) {
    const fixed = await raw(repair.tool, repair.query);
    check('F9: repair returns directory symbols', declarations(fixed).filter(o => o.name === 'stageFile').length > 0, fixed.text.slice(0, 80));
  }
}

// F10 — groupByFile paths resolve against base.
{
  const refs = await call('lspSearch', { uri: path.join(NATIVE, 'crates/runtime/src/process_status.rs'), symbolName: 'is_alive', lineHint: 1, operation: 'references', groupByFile: true });
  const byFile = rowData(refs)?.payload?.byFile ?? [];
  const missing = byFile.map(f => path.resolve(refs.sc?.base ?? '/', f.path)).filter(f => !fs.existsSync(f));
  check('F10: every groupByFile path = base + path exists', byFile.length > 0 && missing.length === 0, missing.slice(0, 2).join(','));
}

// L1 / L2 — large sources: searched and windowed, never a dead end.
{
  const log = path.join(FIXTURES, 'large/huge.log');
  if (fs.existsSync(log)) {
    const count = await call('localSearch', { path: log, searchText: 'level=ERROR', resultView: 'countMatches' });
    const n = collect(rowData(count), o => typeof o.totalOccurrences === 'number')[0]?.totalOccurrences;
    check('L1: 28MB log is searched (exact count)', n === Math.floor(400_000 / 97), `count=${n}`);
    const tail = await call('localFetch', { path: log, startLine: 399_999, endLine: 400_000 });
    check('L2: 28MB log tail window served with exact totals', rowData(tail)?.totalLines === 400_000 && (rowData(tail)?.content ?? '').includes('req=400000'), rowData(tail)?.errorCode ?? '');
    const matchOnHuge = await call('localFetch', { path: log, matchString: 'req=12345 ' });
    check('L2: matchString on a huge file redirects to localSearch', rowData(matchOnHuge)?.errorCode === 'largeSourceWindowOnly' && /localSearch/.test(JSON.stringify(rowData(matchOnHuge)?.hints)), rowData(matchOnHuge)?.errorCode);
  }
}

// Continuation hygiene: every next.* seen above executes without validation errors.
const seen = new Set();
let followed = 0;
const invalid = [];
for (const entry of [...client.log]) {
  for (const h of nextHints(entry.sc)) {
    const key = h.tool + JSON.stringify(h.query);
    if (seen.has(key) || followed >= 40) continue;
    seen.add(key);
    followed += 1;
    const out = await raw(h.tool, h.query);
    if (/Input validation error/.test(out.text)) invalid.push(`${h.path}: ${out.text.slice(0, 120)}`);
  }
}
check(`continuations: ${followed} next.* executed, none invalid`, invalid.length === 0, invalid.slice(0, 2).join(' | '));

const result = summary();
writeResults('workflows', { ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
