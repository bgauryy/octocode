// Regression workflows for every defect fixed in this pass, plus
// continuation hygiene: each `next.*` met along the way must execute.
import fs from 'node:fs';
import { suggestedCall } from './lead-predicates.mjs';
import path from 'node:path';
import { FIXTURES, ROOT, checks, collect, declarations, lspCallers, findHint, nextHints, rowData, rootPath, startServer, writeResults, lspLocations } from './mcp-client.mjs';

const { check, summary } = checks('workflows');
const client = await startServer();
const { call } = client;
const NATIVE = path.join(ROOT, 'packages/octocode-native');
const LSP = path.join(FIXTURES, 'lsp');
const locations = e => lspLocations(e).map(l => rootPath(e, l.path));
const callers = e => lspCallers(e).map(c => `${c.name}@${path.basename(c.path ?? '')}`);

// F1 — TS/JS importers from the declaration side (inferred project, CommonJS alias).
for (const [variant, ext] of [['cjs-noconfig', 'cjs'], ['cjs-jsconfig', 'cjs'], ['esm-noconfig', 'mjs'], ['esm-tsconfig', 'ts']]) {
  const file = path.join(LSP, variant, `a.${ext}`);
  const refs = await call('lspSearch', { path: file, symbolName: 'stage', line: 1, operation: 'references' });
  check(`F1 ${variant}: references reach the importer b.${ext}`, locations(refs).some(f => f.endsWith(`b.${ext}`)), locations(refs).map(f => path.basename(f)).join(','));
  const up = await call('lspSearch', { path: file, symbolName: 'stage', line: 1, operation: 'callers' });
  check(`F1 ${variant}: callers find run`, callers(up).some(c => c.startsWith('run@b.')), callers(up).join(','));
}
{
  const file = path.join(NATIVE, 'scripts/stage-binary.cjs');
  // Anchor on the live declaration line: this is a real repo file that moves.
  const line = fs.readFileSync(file, 'utf8').split('\n').findIndex(l => l.startsWith('function stageFile(')) + 1;
  // coverage.importerScan is a verbose (stats) field, listed only under extras:["stats"].
  const refs = await call('lspSearch', { path: file, symbolName: 'stageFile', line, operation: 'references', extras: ['stats'] });
  const files = new Set(locations(refs).map(f => path.basename(f)));
  check('F1 real repo: stageFile references span build-native.cjs + test', files.has('build-native.cjs') && files.has('build-publication.test.cjs'), [...files].join(','));
  const coverage = rowData(refs)?.coverage;
  check('F1: coverage reports a complete importer scan (extras stats)', coverage?.importerScan === 'complete', JSON.stringify(coverage));
}

// F2 — workspaceSymbol ranks the exact name first.
let rustServer;
{
  const ws = await call('lspSearch', { path: path.join(NATIVE, 'crates/runtime/src/process_status.rs'), operation: 'workspaceSymbol', symbolName: 'is_alive', extras: ['receipts'] });
  rustServer = rowData(ws)?.lsp?.receipt;
  const first = rowData(ws)?.matches?.[0]?.symbolName;
  check('F2: exact workspace symbol ranks first', first === 'is_alive', first);
}

// F3 — a Cargo workspace's declared language takes precedence over its JS wrapper.
{
  const ws = await call('lspSearch', { workspaceRoot: NATIVE, operation: 'workspaceSymbol', symbolName: 'is_alive', extras: ['receipts'] });
  const receipt = rowData(ws)?.lsp?.receipt;
  check('F3: root-only Cargo workspace search uses the Rust anchor server', !!rustServer?.command && receipt?.command === rustServer.command && receipt?.configurationFingerprint === rustServer.configurationFingerprint && receipt?.workspaceFingerprint === rustServer.workspaceFingerprint, JSON.stringify(receipt));
  check('F3: root-only search directly finds the Rust declaration', (rowData(ws)?.matches ?? []).some(i => i.symbolName === 'is_alive' && i.path?.endsWith('.rs')), ws.text.slice(0, 150));
}

// F7 — an ast-grep rule runs as a JSON rule object; a pasted YAML rule file
// and unsupported operators fail clearly, naming the accepted shape.
{
  const rule = { kind: 'function_item', has: { pattern: 'unsafe { $$$ }', stopBy: 'end' } };
  const e = await call('astSearch', { operation: 'match', path: path.join(NATIVE, 'crates/runtime/src'), language: 'rust', resultView: 'files', rule });
  check('F7: a JSON rule object matches', !e.isError && !e.rowErrors && collect(rowData(e), o => typeof o.path === 'string').length > 0, e.text.slice(0, 100));
  const yaml = await call('astSearch', { operation: 'match', path: path.join(NATIVE, 'crates/runtime/src'), language: 'rust', resultView: 'files', rule: 'id: unsafe-fn\nlanguage: rust\nrule:\n  kind: function_item\n' });
  check('F7: a pasted YAML rule file is rejected naming the object fields', yaml.isError && /Expected object/.test(yaml.text) && /\bkind\b/.test(yaml.text), yaml.text.slice(0, 160));
  const bad = await call('astSearch', { operation: 'match', path: path.join(NATIVE, 'crates/runtime/src'), language: 'rust', rule: { pattern: '$A', constraints: {} } });
  check('F7: unsupported constraints still rejected by name', bad.isError && /constraints/.test(bad.text), bad.text.slice(0, 120));
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
    ['grep', await call('localSearch', { path: dir, matchString: 'AKIA' })],
    ['grep PEM body', await call('localSearch', { path: dir, matchString: 'MIIEow' })],
    ['fetch', await call('localFetch', { path: file, fullContent: true })],
  ]) check(`secrets: ${label} never returns the secret`, ![AWS, GHP.slice(0, 14), 'MIIEowIBAAKCAQEA'].some(secret => entry.text.includes(secret)), entry.text.slice(0, 80));
  const right = await call('localFetch', { path: file, matchString: 'ghp_1234' });
  const wrong = await call('localFetch', { path: file, matchString: 'ghp_9999' });
  const observation = (entry, term) => {
    const value = structuredClone(entry.sc);
    for (const data of collect(value, node => Array.isArray(node.hints))) {
      data.hints = data.hints.map(text => {
        const lead = suggestedCall(text);
        if (!lead) return text;
        for (const query of lead.query.queries) {
          if (query.matchString === term) query.matchString = '<input matchString>';
        }
        return lead;
      });
    }
    return JSON.stringify(value);
  };
  check('F8: right and wrong secret guesses differ only in echoed hint inputs', observation(right, 'ghp_1234') === observation(wrong, 'ghp_9999'));
  check('F8: the miss explains redaction', (rowData(right)?.hints ?? []).some(h => h.includes('REDACTED')), JSON.stringify(rowData(right)?.hints));
  fs.rmSync(dir, { recursive: true, force: true });
}

// F9 — directory symbols with language returns an exact, executable repair.
{
  const bad = await call('astSearch', { operation: 'symbols', path: path.join(NATIVE, 'scripts'), language: 'JavaScript' });
  const repair = findHint(bad.sc, 'repair');
  check('F9: repair continuation drops language', !!repair && repair.query.queries.every(query => !Object.hasOwn(query, 'language')), JSON.stringify(rowData(bad)?.hints));
  if (repair) {
    const fixed = await client.follow(repair);
    check('F9: repair returns directory symbols', declarations(fixed).filter(o => o.name === 'stageFile').length > 0, fixed.text.slice(0, 80));
  }
}

// F10 — groupByFile paths resolve against root.
{
  const refs = await call('lspSearch', { path: path.join(NATIVE, 'crates/runtime/src/process_status.rs'), symbolName: 'is_alive', line: 1, operation: 'references', groupByFile: true });
  const byFile = rowData(refs)?.files ?? [];
  const missing = byFile.map(f => rootPath(refs, f.path)).filter(f => !fs.existsSync(f));
  check('F10: every groupByFile path = root + path exists', byFile.length > 0 && missing.length === 0, missing.slice(0, 2).join(','));
}

// L1 / L2 — large sources: searched and windowed, never a dead end.
{
  const log = path.join(FIXTURES, 'large/huge.log');
  if (fs.existsSync(log)) {
    const count = await call('localSearch', { path: log, matchString: 'level=ERROR', resultView: 'countMatches' });
    const n = rowData(count)?.matchCount ?? collect(rowData(count), o => typeof o.matchCount === 'number')[0]?.matchCount;
    check('L1: 28MB log is searched (exact count)', n === Math.floor(400_000 / 97), `count=${n}`);
    const tail = await call('localFetch', { path: log, ranges: ['399999-400000'] });
    check('L2: 28MB log tail window served with exact totals', rowData(tail)?.totalLines === 400_000 && (rowData(tail)?.content ?? '').includes('req=400000'), rowData(tail)?.errorCode ?? '');
    const matchOnHuge = await call('localFetch', { path: log, matchString: 'req=12345 ' });
    check('L2: matchString on a huge file redirects to localSearch', rowData(matchOnHuge)?.errorCode === 'largeSourceWindowOnly' && /localSearch/.test(JSON.stringify(rowData(matchOnHuge)?.hints)), rowData(matchOnHuge)?.errorCode);
  }
}

// Continuation hygiene: every next.* page and hints.* lead seen above executes without validation errors.
const seen = new Set();
let followed = 0;
const invalid = [];
for (const entry of [...client.log]) {
  for (const h of nextHints(entry.sc)) {
    const key = h.tool + JSON.stringify(h.query);
    if (seen.has(key) || followed >= 40) continue;
    seen.add(key);
    followed += 1;
    const out = await client.follow(h);
    if (/Input validation error/.test(out.text)) invalid.push(`${h.path}: ${out.text.slice(0, 120)}`);
  }
}
check(`continuations: ${followed} next.*/hints.* executed, none invalid`, invalid.length === 0, invalid.slice(0, 2).join(' | '));

const result = summary();
writeResults('workflows', { ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
