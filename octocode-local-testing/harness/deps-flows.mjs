// Imports / dependencies / flows between files, cross-checked across tools.
// Explicit imports are verified at importLine. Typed Java same-package
// candidates are verified against package declarations and lexical class uses.
// Bounded syntactic graph coverage is measured against LSP, not called complete.
import path from 'node:path';
import { REPOS, ROOT, checks, collect, nextHints, rowData, sourcePath, sourceView, startServer, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('deps-flows');
const client = await startServer({ env: { OCTOCODE_BETA: '1' } });
const { call, raw } = client;
check('astTopology is exposed with OCTOCODE_BETA=1', client.tools.some(t => t.name === 'astTopology'));

const PROJECTS = [
  { lang: 'TypeScript/TSX', root: 'tsx', scope: 'packages/excalidraw', ext: ['ts', 'tsx'], importKind: 'import_statement', lsp: true },
  { lang: 'Python', root: 'python', scope: 'django/utils', ext: ['py'], importKind: ['import_statement', 'import_from_statement'], lsp: false },
  { lang: 'Go', root: 'go', scope: 'tsdb', ext: ['go'], importKind: 'import_spec', lsp: false },
  { lang: 'Rust', root: 'rust', scope: 'tokio/src/sync', ext: ['rs'], importKind: 'use_declaration', lsp: true, topo: { rustWorkspace: 'cargo' } },
  { lang: 'Java', root: 'java', scope: 'guava/src/com/google/common/collect', ext: ['java'], importKind: 'import_declaration', lsp: false },
  { lang: 'C', root: 'c', scope: 'src', ext: ['h'], importKind: 'preproc_include', lsp: false },
];
const EXCLUDE = ['node_modules', 'target', 'testdata'];

/** Tokens an import of `file` would spell: stem, or the directory for index/mod/__init__/Go packages. */
function tokens(file, lang) {
  const stem = path.basename(file).replace(/\.[^.]+$/, '');
  const dir = path.basename(path.dirname(file));
  if (lang === 'Go') return [dir];
  // A package entry (`packages/common/src/index.ts`) is imported by package name (`@scope/common`).
  if (['index', 'mod', '__init__', 'lib'].includes(stem)) return dir === 'src' ? [path.basename(path.dirname(path.dirname(file))), stem] : [dir, stem];
  return [stem];
}
const results = entry => rowData(entry)?.results ?? [];
async function walkPages(first, key, max = 20) {
  const pages = [first];
  let current = first;
  while (pages.length < max) {
    const h = nextHints(current.sc).find(x => x.path.endsWith(`.${key}`));
    if (!h) break;
    current = await raw(h.tool, h.query);
    if (current.isError) break;
    pages.push(current);
  }
  return pages;
}

/** The import statement starting at `line` (multi-line `import {…} from "x"` included). */
async function lineAt(root, file, line) {
  const f = await call('localFetch', { path: path.join(root, file), startLine: line, endLine: line + 200 });
  const text = sourceView(rowData(f)).text;
  const lines = text.split('\n');
  // The statement ends at its module string: `from 'x'` / `"x"` / `<x>` / `;` / `)`.
  const end = lines.findIndex(l => /from\s+['"]|['"][^'"]*['"]\s*\)?;?\s*$|#include|;\s*$|^\s*\)\s*$/.test(l));
  return lines.slice(0, (end < 0 ? 0 : end) + 1).join('\n');
}

/** Prove sampled edges: the import line in `from` names `to`. */
async function proveEdges(root, lang, edges, limit = 20) {
  let proven = 0;
  const failures = [];
  for (const edge of edges.slice(0, limit)) {
    if (lang === 'Java' && edge.edgeKinds?.includes('java-same-package') && edge.importLine === undefined) {
      const from = await call('localFetch', { path: path.join(root, edge.from), matchString: tokens(edge.to, lang)[0], contextLines: 2 });
      const fromHead = await call('localFetch', { path: path.join(root, edge.from), startLine: 1, endLine: 80 });
      const to = await call('localFetch', { path: path.join(root, edge.to), startLine: 1, endLine: 80 });
      const a = sourceView(rowData(from)).text, b = sourceView(rowData(to)).text;
      const packageOf = s => s.match(/^\s*package\s+([\w.]+)\s*;/m)?.[1];
      const name = tokens(edge.to, lang)[0];
      const declared = packageOf(sourceView(rowData(fromHead)).text);
      if (!from.isError && !to.isError && !fromHead.isError && declared && declared === packageOf(b) && new RegExp(`\\b${name}\\b`).test(a) && new RegExp(`\\b(class|interface|enum|record)\\s+${name}\\b`).test(b)) proven += 1;
      else failures.push(`unverified same-package candidate ${edge.from} ↛ ${edge.to}`);
      continue;
    }
    if (!Number.isInteger(edge.importLine)) { failures.push(`missing importLine/kind ${edge.from} ↛ ${edge.to}`); continue; }
    const text = await lineAt(root, edge.from, edge.importLine);
    if (tokens(edge.to, lang).some(t => text.includes(t))) proven += 1;
    else failures.push(`${edge.from}:${edge.importLine} ↛ ${edge.to} «${text.trim().slice(0, 60)}»`);
  }
  return { proven, sampled: Math.min(limit, edges.length), failures };
}

let projectTopo = {};
async function topo(root, analysis, extra) {
  return call('astTopology', { analysis, path: root, excludeDir: EXCLUDE, ...projectTopo, ...extra });
}

const report = [];
for (const p of PROJECTS) {
  const root = path.join(REPOS, p.root);
  projectTopo = p.topo ?? {};
  const row = { lang: p.lang };
  const listing = await call('structureSearch', { operation: 'files', path: path.join(root, p.scope), extensions: p.ext, detail: 'full', sort: 'lines', limit: 30 });
  const candidates = collect(rowData(listing), o => typeof o.path === 'string' && typeof o.lineCount === 'number')
    // Paths are relative to the response `base` (the workspace for structureSearch).
    .map(o => path.relative(root, path.resolve(listing.sc?.base ?? ROOT, o.path)))
    .filter(f => !/(^|[/_.])(tests?|spec|bench)([/_.]|$)/i.test(f));
  let hub, dependents;
  for (const candidate of candidates.slice(0, 10)) {
    const d = await topo(root, 'dependents', { file: candidate, depth: 1 });
    if (results(d).length >= 2) { hub = candidate; dependents = d; break; }
  }
  check(`${p.lang}: hub with ≥2 dependents`, !!hub, `candidates=${candidates.length}`);
  if (!hub) { report.push(row); continue; }
  row.hub = hub;
  // A diagnostics continuation must deliver the diagnostic entries it pages.
  const diagHint = rowData(dependents)?.next?.nextDiagnostics;
  if (diagHint) {
    const diagPage = await raw(diagHint.tool, diagHint.query);
    // Identical code+message rows are grouped: `files` lists each `path[:line]`.
    const entries = collect(rowData(diagPage)?.coverage, o => (typeof o.file === 'string' || Array.isArray(o.files)) && typeof o.code === 'string');
    check(`${p.lang}: next.nextDiagnostics returns diagnostic entries`, !diagPage.isError && entries.length > 0, `entries=${entries.length} pagination=${JSON.stringify(rowData(diagPage)?.coverage?.diagnosticsPagination ?? {})}`);
  }
  const dependencies = await topo(root, 'dependencies', { file: hub, depth: 1 });
  const depRows = results(dependencies);
  const dependentRows = results(dependents);
  row.deps = depRows.length;
  row.dependents = dependentRows.length;
  row.confidence = rowData(dependencies)?.confidence;
  row.resolution = JSON.stringify(rowData(dependencies)?.summary?.importResolution ?? {}).replace(/"/g, '');

  const inbound = await proveEdges(root, p.lang, dependentRows.map(r => ({ ...r, from: r.file, to: hub })));
  row.dependentsProven = `${inbound.proven}/${inbound.sampled}`;
  check(`${p.lang}: dependent edges proven at their importLine`, inbound.proven === inbound.sampled, inbound.failures.slice(0, 2).join(' | '));
  const outbound = await proveEdges(root, p.lang, depRows.map(r => ({ ...r, from: hub, to: r.file })));
  row.depsProven = `${outbound.proven}/${outbound.sampled}`;
  check(`${p.lang}: dependency edges proven at their importLine`, outbound.proven === outbound.sampled, outbound.failures.slice(0, 2).join(' | '));

  // Syntax view: every importLine the graph reports is an import statement astSearch sees.
  const lang = p.lang === 'TypeScript/TSX' ? (hub.endsWith('.tsx') ? 'TSX' : 'TypeScript') : p.lang;
  const imports = await call('astSearch', { operation: 'match', path: path.join(root, hub), langType: lang, rule: Array.isArray(p.importKind) ? `rule:\n  any:\n${p.importKind.map(k => `    - kind: ${k}\n`).join('')}` : `rule:\n  kind: ${p.importKind}\n`, maxMatchesPerFile: 200 });
  const importLines = new Set(collect(rowData(imports), o => typeof o.value === 'string' && typeof o.line === 'number').flatMap(o => {
    const end = o.endLine ?? o.line; const out = []; for (let l = o.line; l <= end; l++) out.push(l); return out;
  }));
  const explicitRows = depRows.filter(r => !(p.lang === 'Java' && r.edgeKinds?.includes('java-same-package') && r.importLine === undefined));
  const syntaxCovered = explicitRows.filter(r => importLines.has(r.importLine) || p.lang === 'Rust').length;
  row.importSyntax = `${importLines.size} lines; ${syntaxCovered}/${explicitRows.length} explicit imports; ${depRows.length - explicitRows.length} typed same-package candidates`;
  check(`${p.lang}: explicit graph importLines are import syntax (astSearch ${p.importKind})`, !imports.isError && syntaxCovered === explicitRows.length, row.importSyntax);

  // Transitive closure, path, cycles.
  const deep = await topo(root, 'dependencies', { file: hub, depth: 3 });
  const deepFiles = new Set(results(deep).map(r => r.file));
  row.transitive = deepFiles.size;
  check(`${p.lang}: depth 3 ⊇ depth 1`, depRows.every(r => deepFiles.has(r.file)), `d1=${depRows.length} d3=${deepFiles.size}`);
  const far = results(deep).find(r => (r.distance ?? 0) >= 2 || !depRows.some(d => d.file === r.file));
  if (far) {
    const route = await topo(root, 'path', { file: hub, target: far.file });
    const found = results(route)[0];
    const edges = found?.edges ?? [];
    const proof = await proveEdges(root, p.lang, edges, 10);
    row.path = found?.files?.join(' → ');
    check(`${p.lang}: path hub → ${far.file} found and every hop proven`, found?.found && edges.length >= 2 && proof.proven === proof.sampled, proof.failures.slice(0, 2).join(' | ') || row.path);
  }
  const cycles = await topo(root, 'cycles', { path: path.join(root, p.scope), pageSize: 20 });
  row.cycles = rowData(cycles)?.pagination?.totalEntries ?? results(cycles).length;
  check(`${p.lang}: cycles answers`, !cycles.isError, `${cycles.ms}ms total=${row.cycles}`);

  // Identity view: files whose code references an exported hub symbol must depend on the hub (transitively).
  if (p.lsp) {
    const symbols = await call('astSearch', { operation: 'symbols', path: path.join(root, hub) });
    const exported = collect(rowData(symbols), o => typeof o.name === 'string' && typeof o.line === 'number' && o.exported && ['function', 'class', 'constant', 'struct'].includes(o.kind));
    for (const symbol of exported.slice(0, 2)) {
      const refs = await call('lspSearch', { uri: path.join(root, hub), symbolName: symbol.name, lineHint: symbol.line, operation: 'references', pageSize: 25, groupByFile: true });
      const refPages = await walkPages(refs, 'nextPage', 40);
      const refFiles = [...new Set(refPages.flatMap(pg => collect(rowData(pg)?.payload, o => typeof o.path === 'string' || typeof o.uri === 'string').map(o => sourcePath(pg, o, path.join(root, hub)))))];
      const allDependents = await topo(root, 'dependents', { file: hub, depth: 6, pageSize: 25 });
      const depPages = await walkPages(allDependents, 'nextPage', 80);
      const allowed = new Set(depPages.flatMap(pg => results(pg).map(r => r.file)));
      // Files whose imports the graph declares it cannot link (macro-generated
      // Rust paths) are disclosed gaps, not wrong edges.
      const diagPages = await walkPages(allDependents, 'nextDiagnostics', 60);
      const diagnosed = new Set(diagPages.flatMap(pg => collect(rowData(pg)?.coverage, o => (typeof o.file === 'string' || Array.isArray(o.files)) && /macro|unsupported/.test(o.message ?? '')).flatMap(o => o.files ? o.files.map(f => f.replace(/:\d+$/, '')) : [o.file])));
      const inRoot = refFiles.map(f => path.relative(root, f));
      const outside = inRoot.filter(f => !allowed.has(f) && !diagnosed.has(f) && f !== hub && !f.endsWith(path.basename(hub)));
      row.lsp = `${symbol.name}: ${refFiles.length} files, ${outside.length} outside graph`;
      (row.lspComparisons ??= []).push({ symbol: symbol.name, files: refFiles.length, outside, coverage: rowData(allDependents)?.coverage, confidence: rowData(allDependents)?.confidence });
      const partial = rowData(allDependents)?.confidence === 'low' || rowData(allDependents)?.summary?.importResolution?.status === 'partial' || (rowData(allDependents)?.coverage?.imports?.unresolvedInternal ?? 0) > 0;
      check(`${p.lang}: bounded graph/LSP comparison for ${symbol.name} discloses coverage gaps`, !refs.isError && (outside.length === 0 || partial), `outside=${outside.slice(0, 3).join(', ')} partial=${partial}`);
    }
  }
  report.push(row);
}
console.table(report);
const result = summary();
writeResults('deps-flows', { report, ...result, calls: client.log.map(({ text, sc, ...meta }) => meta) });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
