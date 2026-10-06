#!/usr/bin/env node
// Chain sensor (FIX-LIST B1): the six per-tool chain steps (description,
// schema, input, execute, output, communicate) plus the five FIX-LIST
// Acceptance checks (acceptance.mjs C1-C5), over the live MCP server and CLI.
// Rebuilt from the recovered tool-chain-check blob 4af87c11.
//
//   node harness/chain-check.mjs                    # live, markdown to stdout
//   node harness/chain-check.mjs --json             # JSON to stdout
//   node harness/chain-check.mjs --write=baseline   # <out>/chain-check-baseline.{md,json}
//   flags: --tools=localSearch,astSearch --no-live --no-github --clasify (paid) --pages=12
//          --out=<dir> (default .octocode/evals/<date>-chain)
//          --strict=C2,C4 (Acceptance checks that FAIL; the rest REPORT)
//
// run-all lines: PASS/FAIL per strict check and tool; non-strict misses print
// REPORT (not counted), so the sensor never blocks until a check is flipped.
//
// Sources: the core contract (packages/octocode-config/contract/tool-contract.json),
// the live tools/list + instructions (harness MCP client, OCTOCODE_BETA=true),
// `octocode schema <tool>` for CLI-only tools, the native source
// (crates/runtime/src), and live samples: one or two fixed recipe steps per tool
// from harness/competitor-tasks.json, their `next` pages walked (local <= --pages,
// GitHub and CLI <= 3), up to 3 leads replayed verbatim, and one empty + one
// error probe per tool (C2).
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { startServer, cliEntry, ROOT } from './mcp-client.mjs';
import { schemaErrors, verboseFields, loadVerboseRules, isPageName, hintEntries } from './sensors.mjs';
import { CHECKS, checkChainFit, checkRowContract, descriptionLint, rowFieldNames, silentOmissions, surfaceBudget, zeroBasedTexts, zeroCoordinates } from './acceptance.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.join(ROOT, 'packages/octocode-native/crates/runtime/src');
const CONTRACT = path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json');
const TOOL_TYPES = path.join(ROOT, 'packages/octocode-config/contract/tool_types.rs');
const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
const ADDON_DIR = path.join(ROOT, 'packages/octocode-native');

const argv = process.argv.slice(2);
const flag = name => argv.includes(`--${name}`);
const opt = name => argv.find(a => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const LIVE = !flag('no-live');
const GITHUB = LIVE && !flag('no-github');
const PAID = flag('clasify');
const LOCAL_PAGES = Number(opt('pages') ?? 12);
const REMOTE_PAGES = 3;
const LEAD_REPLAYS = 3;
const T = 'octocode-local-testing/repos';
const STRICT = new Set((opt('strict') ?? '').split(',').filter(Boolean));
const OUT = path.resolve(ROOT, opt('out') ?? `.octocode/evals/${new Date().toISOString().slice(0, 10)}-chain`);
const QUOTAS_FILE = path.join(HERE, 'chain-quotas.json');

// ---------- fixed tables (PLAN.md decisions) ----------
/** Native module that owns each ToolId (tools/<module>). */
const MODULE = {
  localSearch: 'local_search', localFetch: 'local_fetch', structureSearch: 'structure_search', astSearch: 'ast_search',
  lspSearch: 'lsp_search', astTopology: 'ast_graph', astRewrite: 'ast_rewrite', ghSearchRepo: 'gh_search_repo',
  ghSearchCode: 'gh_search_code', ghStructure: 'gh_structure', ghGetFileContent: 'gh_get_file_content',
  ghSearchHistory: 'gh_search_history', ghGetHistoryItem: 'gh_get_history_item', artifactSearch: 'artifact_search',
  ghCloneRepo: 'gh_clone_repo', clasify: 'clasify',
};
/** PLAN.md §4 targets: `ToolId::X` references allowed in shared runtime/ + response/ production code. */
const REF_TARGET = {
  localSearch: 5, localFetch: 5, structureSearch: 3, astSearch: 5, lspSearch: 5, astTopology: 3, astRewrite: 3,
  ghSearchRepo: 3, ghSearchCode: 3, ghStructure: 3, ghGetFileContent: 3, ghSearchHistory: 3, ghGetHistoryItem: 5,
  artifactSearch: 3, ghCloneRepo: 3, clasify: 3,
};
/** Duplicate fields for one purpose (PLAN A5, A8, A9, A10): [kept, duplicate, decision]. */
const DUPLICATES = [
  ['exclude', 'excludeDir', 'A5'], ['matchContentLength', 'matchWindow', 'A9'], ['commentPage', 'commentOffset', 'A8'],
  ['qualifiers', 'author', 'A10'], ['qualifiers', 'committer', 'A10'], ['qualifiers', 'label', 'A10'], ['qualifiers', 'branch', 'A10'],
];
/** Same-name behavior notes (ratings/MCP-WHOLE.md §4, PLAN A1–A12). `check` = schema-verifiable alignment. */
const BEHAVIOR = {
  matchString: { note: 'localSearch: search pattern (regex default rust); fetch tools: literal anchor (string or ≤10 list); ghGetHistoryItem: literal hunk filter (string only)', decision: 'A1/A2: keep; docs say so' },
  symbolName: { note: 'astSearch: substring or list, exact-first; lspSearch: exact identifier at lineHint', decision: 'A3: astSearch string behaves like a list (exact-first)' },
  include: { note: 'localSearch/ghGetHistoryItem: path glob; structureSearch: ORed globs; ghStructure: globs or a bare word, raises maxDepth', decision: 'A4: bare word → **/*word* in every tool; include searches recursively in every listing tool' },
  maxDepth: { note: 'ghStructure default 1 (20 with include); structureSearch default 2; localSearch/astSearch hidden recursion', decision: 'A6: 1 = direct children; listing tools default 1' },
  extensions: { note: 'ghSearchCode max 1; structureSearch up to 100', decision: 'A7: ghSearchCode fans out several extensions',
    check: (pub) => { const s = pub.ghSearchCode?.top.extensions; return !s ? null : (s.maxItems ?? Infinity) > 1 ? true : `ghSearchCode extensions maxItems ${s.maxItems}`; } },
  regex: { note: 'localSearch literal|rust|pcre2 default rust; fetch tools literal|rust default literal', decision: 'A2: option, not a rename' },
  ref: { note: 'a read revision; ghGetHistoryItem operation:commit: the commit identity', decision: '—' },
  type: { note: 'artifactSearch: registry; clasify questions[].type: question kind', decision: '—' },
  mainGoal: { note: 'clasify: sent to the judge; other tools: optional brief', decision: 'A11: one shared definition + one instruction line' },
  reasoning: { note: 'as mainGoal', decision: 'A11' },
  minify: { note: 'enums/defaults differ per tool (fetch none, history standard)', decision: '—' },
  contextLines: { note: 'caps differ (10,000 / 10 / 100)', decision: '—' },
};
/** Where a successful (non-final-read) response should lead (CACHING-CONFIG-LEADS §3d.5, AGENT-FLOWS). */
const NATURAL_NEXT = {
  localSearch: ['localFetch', 'lspSearch', 'astSearch'], structureSearch: ['localFetch', 'localSearch', 'astSearch'],
  astSearch: ['localFetch', 'lspSearch'], lspSearch: ['localFetch'], ghSearchRepo: ['ghSearchCode', 'ghStructure'],
  ghSearchCode: ['ghGetFileContent'], ghStructure: ['ghGetFileContent', 'localSearch', 'localFetch'],
  ghSearchHistory: ['ghGetHistoryItem'], artifactSearch: ['ghStructure', 'ghGetFileContent', 'artifactSearch'],
  clasify: ['localFetch', 'ghGetFileContent'], astTopology: ['localFetch', 'lspSearch'], astRewrite: ['astRewrite'],
  ghCloneRepo: ['localSearch', 'structureSearch', 'astSearch', 'localFetch', 'lspSearch'],
  ghGetHistoryItem: ['ghGetHistoryItem', 'ghGetFileContent'],
  localFetch: null, ghGetFileContent: null, // final reads
};
/** Withheld evidence must be a `next` page, never a `hints` lead (PLAN C7, PG6). */
const WITHHELD = /^(readHits\d*|readDeclaration\d*|wholeLines|readBoundedLines|readFullPatches\d*|readUntrimmed)$/;
/** Debug counters that belong under debug:true (PLAN B5). */
const DEBUG_COUNTERS = ['sourceChars', 'returnedChars', 'returnedLines', 'returnedBytes', 'sourceBytes', 'httpStatus', 'retryable'];
/** Fixed live samples: competitor task ids (first matching step); `drop` removes fields the sample does not need. */
const SAMPLES = {
  localSearch: ['L07', 'L01'], localFetch: ['L06', 'L02'], structureSearch: ['L13', 'L12'], astSearch: ['L17', 'L16'],
  lspSearch: ['L18', { id: 'Q3', inline: { tool: 'lspSearch', queries: [{ operation: 'documentSymbols', path: 'octocode-local-testing/harness/mcp-client.mjs' }] } }], ghSearchRepo: ['G09'], ghSearchCode: ['G16'], ghStructure: ['G08'], ghGetFileContent: ['G02'],
  ghSearchHistory: ['G11'], ghGetHistoryItem: ['G13'], artifactSearch: ['G04'], clasify: ['C01'],
  ghCloneRepo: [{ id: 'L20', drop: ['forceRefresh'] }], astTopology: ['L04'], astRewrite: [{ id: 'L19', drop: ['ruleKind'] }],
};

/** C2 probes: one empty and one error row per tool (`null` = no stable probe). */
const CFG = 'packages/octocode-config/src';
const GH = { owner: 'bgauryy', repo: 'octocode-mcp' };
const NO_REPO = { owner: 'bgauryy', repo: 'zz-no-such-repo-octocode' };
const NEVER = 'zzqqxxNeverMatchOctocode';
const PROBES = {
  localSearch: { empty: { path: CFG, matchString: NEVER }, error: { path: 'nope/dir', matchString: 'x' } },
  localFetch: { empty: { path: 'README.md', matchString: NEVER }, error: { path: 'nope.ts' } },
  structureSearch: { empty: { path: CFG, operation: 'files', nameRegex: NEVER }, error: { path: 'nope/dir' } },
  astSearch: { empty: { operation: 'symbols', path: CFG, symbolName: NEVER }, error: { operation: 'symbols', path: 'nope/dir', symbolName: 'x' } },
  lspSearch: { empty: null, error: { path: 'nope/x.ts', symbolName: 'foo', lineHint: 1, operation: 'definition' } },
  astTopology: { empty: null, error: { operation: 'cycles', path: 'nope/dir' } },
  astRewrite: { empty: { path: CFG, language: 'typescript', pattern: `${NEVER}($A)`, rewrite: 'x($A)' }, error: { path: 'nope/dir', language: 'typescript', pattern: 'f($A)', rewrite: 'g($A)' } },
  ghSearchRepo: { empty: { keywords: [NEVER] }, error: null },
  ghSearchCode: { empty: { ...GH, keywords: [NEVER] }, error: { ...NO_REPO, keywords: ['x'] } },
  ghStructure: { empty: { ...GH, path: 'src', include: [`*.${NEVER}`] }, error: { ...NO_REPO } },
  ghGetFileContent: { empty: { ...GH, path: 'README.md', matchString: NEVER }, error: { ...GH, path: 'nope/zz-missing.md' } },
  ghSearchHistory: { empty: { ...GH, operation: 'pullRequests', keywords: [NEVER] }, error: { ...NO_REPO, operation: 'commits' } },
  ghGetHistoryItem: { empty: null, error: { ...GH, operation: 'pullRequest', number: 999999 } },
  artifactSearch: { empty: { type: 'npm', keywords: [NEVER] }, error: { type: 'npm', packageName: NEVER.toLowerCase() } },
  ghCloneRepo: { empty: null, error: { ...NO_REPO } },
  clasify: { empty: null, error: null },
};

/** Walk order: a page reached through a continuation of rank r offers only continuations of rank ≥ r. */
const WALK_ORDER = ['nextPage', 'nextFilePage', 'nextCommitPage', 'nextCommentPage', 'nextReviewPage', 'nextContributorPage', 'nextBranchPage', 'nextTagPage', 'nextLanguagePage', 'nextDiagnosticPage', 'nextMatchPage', 'continuePatch', 'continueBody', 'continueReviewBody', 'continueCommentBody', 'continueBlock', 'continueWalk', 'continueMaterialize', 'continue', 'responsePagination.next'];
const walkRank = name => { const i = WALK_ORDER.indexOf(name); return i < 0 ? WALK_ORDER.length : i; };

// ---------- small helpers ----------
const uniq = xs => [...new Set(xs)];
const B = v => (v === undefined ? 0 : Buffer.byteLength(JSON.stringify(v)));
const short = (s, n = 160) => { s = String(s ?? ''); return s.length > n ? `${s.slice(0, n - 1)}…` : s; };
const addonStamp = () => {
  const f = fs.readdirSync(ADDON_DIR).find(x => /^octocode-native\..+\.node$/.test(x));
  return f ? fs.statSync(path.join(ADDON_DIR, f)).mtime.toISOString() : null;
};

/** Top-level row properties of a query schema (union over oneOf/anyOf variants) → {name: subschema}. */
function topProps(schema) {
  const out = {};
  const visit = s => {
    if (!s || typeof s !== 'object') return;
    for (const [k, v] of Object.entries(s.properties ?? {})) out[k] ??= v;
    for (const branch of [...(s.oneOf ?? []), ...(s.anyOf ?? []), ...(s.allOf ?? [])]) visit(branch);
  };
  visit(schema);
  return out;
}
/** Every property name and enum value under a schema (recursive, $ref-free walk). */
function allNames(schema, names = new Set(), enums = new Set()) {
  const walk = s => {
    if (!s || typeof s !== 'object') return;
    if (Array.isArray(s)) { s.forEach(walk); return; }
    if (s.properties) for (const [k, v] of Object.entries(s.properties)) { names.add(k); walk(v); }
    if (Array.isArray(s.enum)) for (const e of s.enum) if (typeof e === 'string') enums.add(e);
    if (typeof s.const === 'string') enums.add(s.const);
    for (const [k, v] of Object.entries(s)) if (k !== 'properties' && typeof v === 'object') walk(v);
  };
  walk(schema);
  return { names, enums };
}
/** A compact signature of a field schema for cross-tool comparison. */
function signature(s = {}) {
  const types = s.type ? [].concat(s.type) : s.anyOf || s.oneOf ? uniq((s.anyOf ?? s.oneOf).flatMap(b => [].concat(b.type ?? (b.$ref ? 'ref' : 'any')))) : s.$ref ? ['ref'] : ['any'];
  const sig = { type: types.sort().join('|') };
  if (s.enum) sig.enum = [...s.enum].sort().join('|');
  for (const k of ['maximum', 'maxItems', 'default']) if (s[k] !== undefined) sig[k] = s[k];
  return sig;
}
const sigText = sig => Object.entries(sig).map(([k, v]) => (k === 'type' ? v : `${k}:${v}`)).join(' ');

// ---------- Rust source helpers ----------
function stripRust(src) {
  const out = src.split('');
  const n = src.length;
  const blank = (a, b) => { for (let k = a; k < b; k++) if (out[k] !== '\n') out[k] = ' '; };
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') { let j = src.indexOf('\n', i); if (j < 0) j = n; blank(i, j); i = j; continue; }
    if (c === '/' && src[i + 1] === '*') { let d = 1, j = i + 2; while (j < n && d) { if (src.startsWith('/*', j)) { d++; j += 2; } else if (src.startsWith('*/', j)) { d--; j += 2; } else j++; } blank(i, j); i = j; continue; }
    if ((c === 'r' || c === 'b') && !/[\w]/.test(src[i - 1] ?? '')) {
      const m = /^b?r(#*)"/.exec(src.slice(i, i + 260));
      if (m) { const end = src.indexOf(`"${m[1]}`, i + m[0].length); const j = end < 0 ? n : end + 1 + m[1].length; blank(i + 1, j - 1); i = j; continue; }
    }
    if (c === '"') { let j = i + 1; while (j < n && src[j] !== '"') j += src[j] === '\\' ? 2 : 1; blank(i + 1, j); i = j + 1; continue; }
    if (c === "'") { const m = /^'(\\.[^']*|[^'\\])'/.exec(src.slice(i, i + 12)); if (m) { blank(i + 1, i + m[0].length - 1); i += m[0].length; continue; } }
    i++;
  }
  return out.join('');
}
const matchBrace = (s, start) => { let d = 0; for (let k = start; k < s.length; k++) { if (s[k] === '{') d++; else if (s[k] === '}') { d--; if (d === 0) return k; } } return s.length - 1; };
function rustFiles(dir) {
  const out = [];
  const walk = d => { for (const e of fs.readdirSync(d, { withFileTypes: true })) { const p = path.join(d, e.name); if (e.isDirectory()) walk(p); else if (e.name.endsWith('.rs')) out.push(p); } };
  walk(dir);
  return out;
}
/** Production text of every runtime-crate file: comments/strings blanked, #[cfg(test)] items and test files removed. */
function productionSources() {
  const files = rustFiles(SRC);
  const raw = Object.fromEntries(files.map(f => [f, fs.readFileSync(f, 'utf8')]));
  const testFiles = new Set(files.filter(f => /(^|\/)(tests|[a-z0-9_]*_tests?|[a-z0-9_]*_bench)\.rs$/.test(f) || f.includes('/tests/')));
  const stripped = {};
  for (const f of files) stripped[f] = stripRust(raw[f]);
  // `#[cfg(test)] [#[path = "x"]] mod name;` → that file is test code.
  for (const f of files) {
    const re = /#\[cfg\(test\)\]\s*(?:#\[path\s*=\s*"([^"]+)"\]\s*)?(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/g;
    const text = raw[f];
    for (const m of text.matchAll(re)) {
      const base = path.basename(f) === 'mod.rs' || path.basename(f) === 'lib.rs' ? path.dirname(f) : path.join(path.dirname(f), path.basename(f, '.rs'));
      const cand = m[1] ? [path.join(path.dirname(f), m[1])] : [path.join(base, `${m[2]}.rs`), path.join(base, m[2], 'mod.rs')];
      for (const c of cand) if (raw[c] !== undefined) testFiles.add(c);
    }
  }
  const prod = {};
  for (const f of files) {
    if (testFiles.has(f)) continue;
    let s = stripped[f];
    // Blank #[cfg(test)]-attributed items (mod blocks, fns, impls).
    const re = /#\[cfg\((?:test|all\(test[^)]*\)|any\(test[^)]*\))\)\]/g;
    let m;
    while ((m = re.exec(s))) {
      const open = s.indexOf('{', m.index);
      const semi = s.indexOf(';', m.index);
      if (semi >= 0 && (open < 0 || semi < open)) { s = s.slice(0, m.index) + s.slice(m.index, semi + 1).replace(/[^\n]/g, ' ') + s.slice(semi + 1); continue; }
      if (open < 0) break;
      const close = matchBrace(s, open);
      s = s.slice(0, m.index) + s.slice(m.index, close + 1).replace(/[^\n]/g, ' ') + s.slice(close + 1);
    }
    prod[f] = s;
  }
  return { raw, prod };
}
const rel = f => path.relative(SRC, f);
const lineOf = (text, index) => text.slice(0, index).split('\n').length;
/** Parameters of the fn whose `fn` keyword is at `index`: [{name, type}]. */
function fnParams(text, index) {
  let i = text.indexOf('(', index);
  // Skip generic params `<...>` before the parameter list.
  const lt = text.indexOf('<', index);
  if (lt >= 0 && lt < i) { let d = 0, k = lt; for (; k < text.length; k++) { if (text[k] === '<') d++; else if (text[k] === '>') { d--; if (d === 0) break; } } i = text.indexOf('(', k); }
  let d = 0, k = i;
  for (; k < text.length; k++) { if (text[k] === '(') d++; else if (text[k] === ')') { d--; if (d === 0) break; } }
  const body = text.slice(i + 1, k);
  const parts = []; let cur = ''; let depth = 0;
  for (const ch of body) { if ('<([{'.includes(ch)) depth++; if ('>)]}'.includes(ch)) depth--; if (ch === ',' && depth === 0) { parts.push(cur); cur = ''; } else cur += ch; }
  if (cur.trim()) parts.push(cur);
  return parts.map(p => p.replace(/\s+/g, ' ').trim()).filter(Boolean).map(p => { const c = p.indexOf(':'); return c < 0 ? { name: p, type: '' } : { name: p.slice(0, c).trim(), type: p.slice(c + 1).trim() }; });
}

// ---------- step 3 (input) and step 4 (execute): static ----------
function staticChecks(tools, generated) {
  const { raw, prod } = productionSources();
  const toolsDir = path.join(SRC, 'tools');
  const out = {};
  // ToolId::X counts in shared runtime/ + response/ production code.
  const shared = Object.keys(prod).filter(f => /^(runtime|response)\//.test(rel(f)));
  const refCount = {}; const refSites = {};
  for (const f of shared) for (const m of prod[f].matchAll(/ToolId::(\w+)/g)) {
    const v = m[1]; refCount[v] = (refCount[v] ?? 0) + 1; (refSites[v] ??= {})[rel(f)] = ((refSites[v] ??= {})[rel(f)] ?? 0) + 1;
  }
  const owners = {};
  for (const [tool, mod] of Object.entries(MODULE)) (owners[mod] ??= []).push(tool);
  for (const tool of tools) {
    const variant = tool[0].toUpperCase() + tool.slice(1);
    const mod = MODULE[tool];
    const dir = path.join(toolsDir, mod);
    const modFiles = Object.keys(prod).filter(f => f.startsWith(dir + path.sep) || f === `${dir}.rs`);
    // ---- step 3: entry signatures ----
    const input = { ok: true, reasons: [], entries: [] };
    const generatedType = `${variant}Query`;
    for (const f of modFiles) {
      const text = prod[f];
      for (const m of text.matchAll(/\bpub(?:\(crate\))?\s+(?:async\s+)?fn\s+(execute\w*)/g)) {
        if (/_inner$/.test(m[1])) continue;
        const params = fnParams(text, m.index + m[0].lastIndexOf("fn "));
        const q = params.find(p => /^(query|request|q|row|input|state|args)$/.test(p.name)) ?? params.find(p => /Query|Request|Value/.test(p.type));
        if (!q) continue;
        const where = `${rel(f)}:${lineOf(text, m.index)}`;
        const bare = q.type.replace(/<[^]*>/, '').trim();
        const base = (bare.match(/([A-Z]\w*)\s*$/) ?? [])[1] ?? bare;
        const entry = { fn: m[1], param: `${q.name}: ${q.type}`, where };
        // A module shared by several tools: keep only this tool's entry (its type names the tool).
        if (owners[mod].length > 1 && !new RegExp(`\\b${variant}`).test(q.type)) continue;
        if (/\bValue\b/.test(q.type)) { input.ok = false; input.reasons.push(`\`${m[1]}(${q.name}: ${q.type})\` raw Value (${where})`); entry.kind = 'value'; }
        else if (generated.has(base)) entry.kind = 'generated';
        else {
          // A hand-written wrapper is fine when it holds the generated query and no raw Value.
          const def = Object.entries(prod).map(([df, t]) => [df, t, new RegExp(`\\bstruct\\s+${base}\\b[^{;]*\\{`).exec(t)]).find(x => x[2]);
          if (def) {
            const body = def[1].slice(def[2].index, matchBrace(def[1], def[1].indexOf('{', def[2].index)) + 1);
            const holds = new RegExp(`\\b${generatedType}\\b`).test(body);
            const rawValue = /:\s*(?:serde_json::)?Value\b/.test(body);
            entry.kind = holds ? (rawValue ? 'wrapper+value' : 'wrapper') : 'hand-written';
            if (!holds) { input.ok = false; input.reasons.push(`\`${m[1]}\` takes hand-written \`${base}\`, not generated \`${generatedType}\` (${where})`); }
            else if (rawValue) { input.ok = false; input.reasons.push(`\`${base}\` keeps a raw Value beside \`${generatedType}\` (${rel(def[0])})`); }
          } else entry.kind = 'unknown';
        }
        input.entries.push(entry);
      }
    }
    if (!input.entries.length) { input.ok = false; input.reasons.push(`no \`execute*\` entry found in tools/${mod}`); }
    const validators = modFiles.flatMap(f => [...prod[f].matchAll(/\bfn\s+(validate\w*)/g)].map(m => `${m[1]} (${rel(f)}:${lineOf(prod[f], m.index)})`));
    input.toolValidators = validators;
    out[tool] = { input };
    // ---- step 4: layout + shared refs ----
    const exec = { ok: true, reasons: [] };
    if (!fs.existsSync(dir)) { exec.ok = false; exec.reasons.push(`no tools/${mod}/ module`); }
    if (owners[mod].length > 1) { exec.ok = false; exec.reasons.push(`tools/${mod}/ hosts ${owners[mod].length} tools (${owners[mod].join(', ')})`); }
    if (fs.existsSync(dir) && raw[`${dir}.rs`] !== undefined) { exec.ok = false; exec.reasons.push(`root file tools/${mod}.rs beside tools/${mod}/ (${raw[`${dir}.rs`].split('\n').length} lines)`); }
    if (tool === 'clasify') {
      const rt = Object.keys(raw).filter(f => /^runtime\/clasify_\w+\.rs$/.test(rel(f)));
      if (rt.length) { exec.ok = false; exec.reasons.push(`tool logic in runtime/: ${rt.length} runtime/clasify_*.rs files (${rt.reduce((a, f) => a + raw[f].split('\n').length, 0)} lines)`); }
    }
    const refs = refCount[variant] ?? 0;
    exec.sharedRefs = refs;
    exec.refTarget = REF_TARGET[tool];
    exec.refSites = Object.entries(refSites[variant] ?? {}).sort((a, b) => b[1] - a[1]).map(([f, n]) => `${f}×${n}`);
    if (refs > REF_TARGET[tool]) { exec.ok = false; exec.reasons.push(`ToolId::${variant} ×${refs} in runtime/+response/ (target ≤ ${REF_TARGET[tool]}; top: ${exec.refSites.slice(0, 3).join(', ')})`); }
    exec.moduleLines = modFiles.reduce((a, f) => a + raw[f].split('\n').length, 0);
    out[tool].execute = exec;
  }
  return out;
}

// ---------- step 1 (description) and step 2 (schema) ----------
function descriptionCheck(tool, pub, contractTool, toolNames, outputNames, leadNames) {
  const desc = pub.description ?? '';
  const res = { ok: true, reasons: [], bytes: Buffer.byteLength(desc) };
  const { names: pubNames, enums } = allNames(pub.inputSchema);
  const contractTop = topProps(contractTool.querySchema);
  const hidden = Object.keys(contractTop).filter(k => !pubNames.has(k));
  const allowed = new Set([...pubNames, ...enums, ...toolNames, ...outputNames, ...leadNames]);
  const flagged = new Set();
  // Hidden contract fields named as words in the description.
  for (const h of hidden) if (new RegExp(`(^|[^\\w.])${h}(?![\\w])`).test(desc)) { flagged.add(h); res.reasons.push(`names unpublished field \`${h}\``); }
  // camelCase identifiers and `field:value` tokens must be published names, enum values, tools, output keys or leads.
  const tokens = uniq([...desc.matchAll(/(?<![\w.])([a-z][a-z0-9]*[A-Z][A-Za-z0-9]*)(?![\w])/g)].map(m => m[1]).concat([...desc.matchAll(/(?<![\w.])([a-z][A-Za-z]*):(?=["[{\d<])/g)].map(m => m[1])));
  for (const tok of tokens) if (!allowed.has(tok) && !flagged.has(tok)) { flagged.add(tok); res.reasons.push(`unknown token \`${tok}\``); }
  // Dotted output/lead paths: the last segment must exist in the output schema or the lead kinds.
  for (const m of desc.matchAll(/\b(hints|next|location|results)\.([A-Za-z]+)\b/g)) if (!outputNames.has(m[2]) && !leadNames.has(m[2]) && m[2] !== 'X') res.reasons.push(`path \`${m[0]}\` not in the output schema`);
  const neighbors = toolNames.filter(t => t !== tool && new RegExp(`\\b${t}\\b`).test(desc));
  res.neighbors = neighbors;
  if (!neighbors.length) res.reasons.push('names no neighbor tool (before/after)');
  res.ok = res.reasons.length === 0;
  return res;
}

function schemaChecks(tools, pub, contract) {
  const out = {};
  // Cross-tool same-name table over published top-level fields.
  const byField = {};
  for (const t of tools) for (const [k, s] of Object.entries(pub[t].top)) (byField[k] ??= {})[t] = signature(s);
  const table = Object.entries(byField).filter(([, m]) => Object.keys(m).length > 1).map(([field, m]) => {
    const sigs = uniq(Object.values(m).map(sigText));
    const b = BEHAVIOR[field];
    const verdict = b?.check ? b.check(pub) : null;
    return { field, tools: Object.keys(m), schemaVariants: sigs.length, signatures: Object.fromEntries(Object.entries(m).map(([t, s]) => [t, sigText(s)])), note: b?.note ?? '', decision: b?.decision ?? '', aligned: verdict === null ? (sigs.length === 1 ? 'same schema' : 'schema differs') : verdict === true ? 'aligned' : `open: ${verdict}` };
  }).sort((a, b) => b.tools.length - a.tools.length || a.field.localeCompare(b.field));
  for (const t of tools) {
    const ct = contract.tools.find(x => x.name === t);
    const full = allNames(ct.querySchema).names;
    const res = { ok: true, reasons: [], duplicates: [] };
    for (const [kept, dup, d] of DUPLICATES) if (full.has(kept) && full.has(dup)) { res.duplicates.push(`${dup}+${kept}`); res.reasons.push(`duplicate \`${dup}\` beside \`${kept}\` (${d})`); }
    // clasify's brief is sent to the judge (a distinct meaning), so only the other tools' copies count.
    for (const f of ['mainGoal', 'reasoning']) if (pub[t].top[f]?.description && !pub[t].cliOnly && t !== 'clasify') res.reasons.push(`per-tool \`${f}\` note "${short(pub[t].top[f].description, 40)}" (A11)`);
    for (const row of table) if (row.tools.includes(t) && row.aligned.startsWith('open')) res.reasons.push(`\`${row.field}\` ${row.aligned} (${row.decision})`);
    if (t === 'clasify') {
      const q = ct.querySchema?.properties?.resources?.items;
      // Resolve a `$defs` ref: the read-query schema is shared by reference.
      let query = topProps(q).query ?? {};
      const ref = typeof query.$ref === 'string' && query.$ref.match(/^#\/\$defs\/(.+)$/);
      if (ref) query = { ...ct.querySchema.$defs?.[ref[1]], ...query };
      const qs = JSON.stringify(query);
      if (!/"queries"|\{queries:\[/.test(qs)) res.reasons.push('resources[].query takes a bare row only, not the {queries:[row]} lead envelope (A12)');
    }
    res.ok = res.reasons.length === 0;
    out[t] = res;
  }
  return { perTool: out, table };
}

// ---------- live sampling ----------
function resolveArgs(value) {
  if (typeof value === 'string') return value.replaceAll('{{ROOT}}', ROOT).replaceAll('{{T}}', T);
  if (Array.isArray(value)) return value.map(resolveArgs);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, resolveArgs(v)]));
  return value;
}
function sampleCalls(tool, tasks) {
  return (SAMPLES[tool] ?? []).map(s => (typeof s === 'string' ? { id: s } : s)).flatMap(({ id, drop = [], inline }) => {
    if (inline) return [{ id, surface: 'mcp', args: { queries: inline.queries } }];
    const task = tasks.find(x => x.id === id);
    const step = task?.octocode.find(st => (st.tool ?? st.cli) === tool && !/\{\{(prev|hint):/.test(JSON.stringify(st)));
    if (!step) return [];
    const rows = (step.queries ?? [step.args]).map(r => { const row = resolveArgs(r); for (const k of drop) delete row[k]; return row; });
    return [{ id, surface: step.cli ? 'cli' : 'mcp', args: { ...(step.envelope ?? {}), queries: rows } }];
  });
}
/** Row-level views of one response: [{data, warnings, pages:[{name,hint}], leads:[{name,hint}]}]. */
function rowsOf(sc, tool) {
  if (!sc || typeof sc !== 'object') return [];
  const env = [...(Array.isArray(sc.warnings) ? sc.warnings : [])];
  const rows = Array.isArray(sc.results) ? sc.results : Array.isArray(sc.queries) ? sc.queries : [sc];
  const out = rows.map(row => {
    const data = row?.data ?? row ?? {};
    const pages = hintEntries(data.next ?? row?.next, 'next').filter(e => isPageName(e.name, tool) || e.name === 'next');
    const leads = hintEntries(data.hints ?? row?.hints, 'hints').concat(hintEntries(data.next ?? row?.next, 'next').filter(e => !isPageName(e.name, tool) && e.name !== 'next'));
    const warnings = [...(Array.isArray(data.warnings) ? data.warnings : []), ...(Array.isArray(row?.warnings) ? row.warnings : []), ...env].filter(w => typeof w === 'string');
    return { data, status: row?.status, warnings, pages, leads, isPartial: data.isPartial === true || row?.isPartial === true, pagination: data.pagination };
  });
  const rp = sc.responsePagination;
  if (rp?.next && typeof rp.next.tool === 'string') out.push({ data: {}, warnings: env, pages: [{ name: 'responsePagination.next', hint: rp.next }], leads: [], envelope: true, pagination: rp });
  return out;
}
const isDateTime = s => /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/.test(s);
function stringLeaves(v, out = [], at = '') {
  if (typeof v === 'string') out.push([at, v]);
  else if (v && typeof v === 'object') for (const [k, c] of Object.entries(v)) { if (k === 'hints' || k === 'next' || k === 'query') continue; stringLeaves(c, out, `${at}.${k}`); }
  return out;
}
function pathValues(obj, dotted) {
  const segs = dotted.split('.');
  let cur = [obj];
  for (const seg of segs) {
    const arr = seg.endsWith('[]');
    const key = arr ? seg.slice(0, -2) : seg;
    cur = cur.flatMap(o => (o && typeof o === 'object' && key in o ? [o[key]] : [])).flatMap(v => (arr ? (Array.isArray(v) ? v : []) : [v]));
  }
  return cur.filter(v => v !== undefined);
}

/** C1/C3/C4 on one live page (sample page, replay, or probe). */
function acceptPage(acc, tool, label, e) {
  const sc = e.raw ?? e.sc;
  if (!sc || typeof sc !== 'object') return;
  for (const r of checkChainFit(sc, tool, { inputNames: t => contractInputs[t] ?? null, published: t => publishedFields[t] ?? null, toolNames: ALL_TOOLS })) acc.C1.push(`${label}: ${r}`);
  for (const r of zeroCoordinates(sc)) acc.C3.push(`${label}: ${r}`);
  for (const r of silentOmissions(sc)) acc.C4.push(`${label}: ${r}`);
}

async function liveChecks(tools, contract, tasks) {
  const server = await startServer({ cwd: ROOT, env: { OCTOCODE_BETA: 'true' } });
  const rules = loadVerboseRules();
  const cliOnly = new Set(contract.tools.filter(t => t.cliOnly).map(t => t.name));
  const remote = new Set(contract.tools.filter(t => t.family === 'remote' || t.family === 'github').map(t => t.name));
  let ghCalls = 0;
  const call = async (tool, args) => {
    if (remote.has(tool) || tool === 'artifactSearch') ghCalls++;
    if (tool === 'ghSearchCode') await new Promise(r => setTimeout(r, 2500));
    if (cliOnly.has(tool)) { const e = cliEntry(tool, args, { cwd: ROOT, timeout: 180_000 }); e.raw = e.sc; return e; }
    return server.raw(tool, args, '', { keepRaw: true });
  };
  const out = {};
  for (const tool of tools) {
    const res = { acceptance: { C1: [], C2: [], C3: [], C4: [] }, output: { ok: true, reasons: [], verbose: {}, paginationKeys: [], dates: [] }, communicate: { ok: true, reasons: [], pages: 0, pagesWithNext: 0, missingMore: [], terminal: 0, staleTerminal: [], hintsMax: 0, withheld: [], replays: [], natural: null, naturalBySample: [] }, samples: [] };
    const remoteTool = remote.has(tool) || tool === 'artifactSearch' || tool === 'ghCloneRepo';
    if (tool === 'clasify' && !PAID) { res.skipped = 'clasify is paid (pass --clasify)'; out[tool] = res; continue; }
    if (remoteTool && !GITHUB) { res.skipped = 'GitHub disabled (--no-github)'; out[tool] = res; continue; }
    const calls = sampleCalls(tool, tasks);
    if (!calls.length) { res.skipped = 'no sample'; out[tool] = res; continue; }
    const leadsSeen = new Map();
    for (const sample of calls) {
      const cap = remoteTool || cliOnly.has(tool) ? REMOTE_PAGES : LOCAL_PAGES;
      const pages = [];
      const queue = [{ tool, args: sample.args, via: 'sample', rank: 0 }];
      const seen = new Set();
      while (queue.length && pages.length < cap) {
        const step = queue.shift();
        const key = JSON.stringify(step.args);
        if (seen.has(key) || (step.args?.queries ?? []).some(q => q?.apply === true)) continue;
        seen.add(key);
        const e = await call(step.tool, step.args);
        pages.push({ e, via: step.via });
        if (e.isError) break;
        // Ordered walk (outer pages before inner ones); expand*/retry/restart* re-read, so they are not walked.
        for (const r of rowsOf(e.sc, tool)) for (const p of r.pages) if (!/^(expand|retry|restart)/.test(p.name) && (p.name === 'responsePagination.next' || walkRank(p.name) >= step.rank)) queue.push({ tool: p.hint.tool, args: p.hint.query, via: p.name, rank: p.name === 'responsePagination.next' ? step.rank : walkRank(p.name) });
      }
      const exhausted = queue.length === 0;
      const sampleRec = { id: sample.id, surface: cliOnly.has(tool) ? 'cli' : 'mcp', pages: pages.length, exhausted, bytes: pages.map(p => p.e.bytes), ms: pages.map(p => p.e.ms), errors: pages.filter(p => p.e.isError).map(p => short(p.e.text, 200)) };
      res.samples.push(sampleRec);
      const keySets = [];
      pages.forEach(({ e, via }, i) => {
        const label = `${sample.id} p${i + 1}${i ? ` (${via})` : ''}`;
        const se = schemaErrors(e);
        if (se.count) { res.output.ok = false; res.output.reasons.push(`${label}: schema error ${se.codes.join(',')} ${short(se.detail, 120)}`); }
        // Output: verbose fields (contract verbosePaths + harness rules + B5 counters).
        const ct = contract.tools.find(x => x.name === tool);
        const queries = e.args?.queries ?? [];
        if (!queries.some(q => q?.debug === true) && e.raw) {
          for (const vp of ct.verbosePaths ?? []) { const vals = pathValues(e.raw, vp); if (vals.length) { const k = vp.replace(/^results\[\]\./, ''); res.output.verbose[k] = (res.output.verbose[k] ?? 0) + vals.length; } }
          for (const [rule, v] of Object.entries(verboseFields(e, rules))) if (rule !== 'debugRows') res.output.verbose[`rule:${rule}`] = (res.output.verbose[`rule:${rule}`] ?? 0) + v.count;
          const keyWalk = (n, inLead) => { if (!n || typeof n !== 'object') return; for (const [k, v] of Object.entries(n)) { if (k === 'hints' || k === 'next' || k === 'query') continue; if (!inLead && DEBUG_COUNTERS.includes(k)) res.output.verbose[k] = (res.output.verbose[k] ?? 0) + 1; keyWalk(v, inLead); } };
          keyWalk(e.raw, false);
        }
        // Output: dates in UTC.
        for (const [at, s] of stringLeaves(e.sc)) if (isDateTime(s) && !/Z$/.test(s)) res.output.dates.push(`${label} ${at}=${s}`);
        // Communicate.
        acceptPage(res.acceptance, tool, label, e);
        const rows = rowsOf(e.sc, tool);
        res.communicate.pages++;
        const rowPages = rows.filter(r => r.pages.length);
        keySets.push(rows.filter(r => !r.envelope).map(r => (r.pagination && typeof r.pagination === 'object' ? Object.keys(r.pagination).sort().join(',') : '∅')).join(' | '));
        if (rowPages.length) {
          res.communicate.pagesWithNext++;
          const missing = rowPages.filter(r => !r.envelope).filter(r => !r.warnings.some(w => /\bmore\b/i.test(w) || r.pages.some(p => w.includes(`next.${p.name}`))));
          if (missing.length) res.communicate.missingMore.push(`${label}: next.${uniq(missing.flatMap(r => r.pages.map(p => p.name))).join(',')} without an "N more" line`);
        } else if (!e.isError) {
          res.communicate.terminal++;
          const stale = rows.filter(r => r.isPartial || r.warnings.some(w => /follow (next|responsePagination)|\d+ more\b/i.test(w)));
          if (stale.length && exhausted) res.communicate.staleTerminal.push(`${label}: ${stale.map(r => (r.isPartial ? 'isPartial' : short(r.warnings.find(w => /more|follow/i.test(w)), 80))).join('; ')}`);
        }
        for (const r of rows) {
          const hintLeads = r.leads.filter(l => l.name !== 'text');
          res.communicate.hintsMax = Math.max(res.communicate.hintsMax, hintLeads.length);
          if (hintLeads.length > 2) res.communicate.reasons.push(`${label}: ${hintLeads.length} leads in one menu (${hintLeads.map(l => l.name).join(',')})`);
          for (const l of hintLeads) {
            if (WITHHELD.test(l.name)) res.communicate.withheld.push(`${label}: withheld evidence \`${l.name}\` filed as a lead`);
            const k = `${l.name}:${l.hint.tool}`; // one replay per lead name keeps remote calls modest
            if (!leadsSeen.has(k)) leadsSeen.set(k, { name: l.name, hint: l.hint, from: label });
          }
        }
        if (i === 0 && NATURAL_NEXT[tool] !== undefined) {
          const want = NATURAL_NEXT[tool];
          const success = !e.isError && rows.some(r => !r.envelope && (r.status ?? 'ok') !== 'error' && r.status !== 'empty');
          let verdict;
          if (want === null) verdict = 'n/a (final read)';
          else if (!success) verdict = 'n/a (not a success)';
          else {
            const found = rows.flatMap(r => [...r.leads, ...r.pages]).filter(l => want.includes(l.hint.tool));
            verdict = found.length ? `yes (${uniq(found.map(l => `${l.name}→${l.hint.tool}`)).join(', ')})` : `no lead to ${want.join('|')}`;
          }
          (res.communicate.naturalBySample ??= []).push(`${sample.id}: ${verdict}`);
        }
      });
      const distinct = uniq(keySets.filter(Boolean));
      res.output.paginationKeys.push(`${sample.id}: ${distinct.length > 1 ? `DRIFT ${distinct.map(s => `{${s}}`).join(' → ')}` : distinct[0] ? `{${distinct[0]}}` : '—'}`);
      if (distinct.length > 1) { res.output.ok = false; res.output.reasons.push(`${sample.id}: pagination keys change across pages: ${distinct.map(s => `{${s}}`).join(' → ')}`); }
    }
    // Replay each lead verbatim (up to LEAD_REPLAYS; clasify leads only with --clasify; GitHub targets only with GitHub on).
    for (const { name, hint, from } of [...leadsSeen.values()].slice(0, LEAD_REPLAYS)) {
      // Never replay a lead that writes (astRewrite apply): it would edit the corpus.
      if ((hint.query?.queries ?? [hint.query]).some(q => q?.apply === true)) { res.communicate.replays.push({ name, tool: hint.tool, ok: null, detail: 'skipped (writes files)' }); continue; }
      if (hint.tool === 'clasify' && !PAID) { res.communicate.replays.push({ name, tool: hint.tool, ok: null, detail: 'skipped (paid)' }); continue; }
      if ((remote.has(hint.tool) || hint.tool === 'artifactSearch') && !GITHUB) { res.communicate.replays.push({ name, tool: hint.tool, ok: null, detail: 'skipped (GitHub off)' }); continue; }
      const e = await call(hint.tool, hint.query);
      acceptPage(res.acceptance, hint.tool, `replay ${name}`, e);
      const se = schemaErrors(e);
      const ok = se.count === 0 && !(e.isError && !e.sc);
      res.communicate.replays.push({ name, tool: hint.tool, from, ok, bytes: e.bytes, ms: e.ms, detail: ok ? (e.rowErrors ? 'ran (row error)' : 'ran') : short(se.detail || e.text, 160) });
    }
    // C2: one empty + one error probe through the tool's real path.
    res.probes = {};
    for (const kind of ['empty', 'error']) {
      const row = PROBES[tool]?.[kind];
      if (!row) { res.probes[kind] = 'no probe'; continue; }
      const e = await call(tool, { queries: [row] });
      const reasons = checkRowContract(e.sc, kind, { schemaErr: schemaErrors({ ...e, raw: e.raw ?? e.sc }), isError: e.isError, text: e.text });
      res.probes[kind] = reasons.length ? reasons.join('; ') : 'ok';
      for (const r of reasons) res.acceptance.C2.push(`${kind} probe: ${r}`);
      acceptPage(res.acceptance, tool, `${kind} probe`, e);
    }
    // Verdicts.
    const vb = Object.entries(res.output.verbose);
    if (vb.length) { res.output.ok = false; res.output.reasons.push(`verbose without debug: ${vb.map(([k, n]) => `${k}×${n}`).join(', ')}`); }
    if (res.output.dates.length) { res.output.ok = false; res.output.reasons.push(`non-UTC dates: ${res.output.dates.slice(0, 2).join('; ')}`); }
    const c = res.communicate;
    if (c.missingMore.length) c.reasons.push(`${c.missingMore.length}/${c.pagesWithNext} pages with next lack an "N more" line (${short(c.missingMore[0], 90)})`);
    if (c.staleTerminal.length) c.reasons.push(`last page not clean: ${c.staleTerminal[0]}`);
    if (c.withheld.length) c.reasons.push(c.withheld[0] + (c.withheld.length > 1 ? ` (+${c.withheld.length - 1})` : ''));
    const failedReplays = c.replays.filter(r => r.ok === false);
    if (failedReplays.length) c.reasons.push(`replay failed: ${failedReplays.map(r => `${r.name}→${r.tool}: ${r.detail}`).join('; ')}`);
    c.natural = (c.naturalBySample ?? []).join('; ') || null;
    const noNatural = (c.naturalBySample ?? []).filter(x => x.includes(': no lead'));
    if (noNatural.length) c.reasons.push(`natural next missing on success: ${noNatural.join('; ')}`);
    if (!c.terminal && res.samples.every(s => !s.exhausted) && c.pagesWithNext) c.notes = ['last page not reached within the page cap (not judged)'];
    c.ok = c.reasons.length === 0;
    out[tool] = res;
  }
  const instructions = server.init?.instructions ?? '';
  server.close();
  return { perTool: out, ghCalls, instructionsBytes: Buffer.byteLength(instructions) };
}

// ---------- main ----------
const contract = JSON.parse(fs.readFileSync(CONTRACT, 'utf8'));
const ALL_TOOLS = contract.tools.map(t => t.name);
const TOOLS = opt('tools')?.split(',') ?? ALL_TOOLS;
const tasks = JSON.parse(fs.readFileSync(path.join(ROOT, 'octocode-local-testing/harness/competitor-tasks.json'), 'utf8')).tasks;
const generated = new Set([...fs.readFileSync(TOOL_TYPES, 'utf8').matchAll(/pub (?:struct|enum|type) (\w+)/g)].map(m => m[1]));
const startedAt = new Date().toISOString();
const addonBefore = addonStamp();

// Published view: live tools/list for MCP tools, `octocode schema` for CLI-only tools.
const list = await startServer({ cwd: ROOT, env: { OCTOCODE_BETA: 'true' } });
const listed = Object.fromEntries(list.tools.map(t => [t.name, t]));
const toolsListBytes = JSON.stringify(list.tools).length;
const instructionsBytes = (list.init?.instructions ?? '').length;
list.close();
// B8 measures the default surface: beta off, clasify registered only with a key.
const plain = await startServer({ cwd: ROOT, env: { OCTOCODE_BETA: 'false' } });
const defaultSurface = Object.fromEntries(plain.tools.map(t => [t.name, JSON.stringify(t).length]));
const defaultInstructionsBytes = (plain.init?.instructions ?? '').length;
plain.close();
const pub = {};
for (const t of ALL_TOOLS) {
  const ct = contract.tools.find(x => x.name === t);
  if (listed[t]) pub[t] = { description: listed[t].description, inputSchema: listed[t].inputSchema, cliOnly: false, bytes: JSON.stringify(listed[t]).length };
  else {
    const r = spawnSync(process.execPath, [CLI, 'schema', t], { cwd: ROOT, encoding: 'utf8', env: { ...process.env, OCTOCODE_BETA: 'true' }, maxBuffer: 64 << 20 });
    let j = {}; try { j = JSON.parse(r.stdout); } catch {}
    pub[t] = { description: j.description ?? '', inputSchema: j.inputSchema ?? {}, cliOnly: !!ct.cliOnly, bytes: null };
  }
  const items = pub[t].inputSchema?.properties?.queries?.items ?? {};
  pub[t].top = topProps(items);
}
/** Full contract row fields (continuation fields included) and published row fields, per tool. */
const contractInputs = Object.fromEntries(contract.tools.map(t => [t.name, rowFieldNames(t.querySchema)]));
const publishedFields = Object.fromEntries(ALL_TOOLS.map(t => [t, new Set(Object.keys(pub[t].top))]));
const leadNames = new Set([...contract.continuationChannels.kinds.leads, ...contract.continuationChannels.kinds.pages]);
const results = {};
const schema = schemaChecks(TOOLS, pub, contract);
const statics = staticChecks(TOOLS, generated);
for (const t of TOOLS) {
  const ct = contract.tools.find(x => x.name === t);
  const outputNames = allNames(ct.outputSchema).names;
  results[t] = { description: descriptionCheck(t, pub[t], ct, ALL_TOOLS, outputNames, leadNames), schema: schema.perTool[t], input: statics[t].input, execute: statics[t].execute };
}
let live = null;
if (LIVE) {
  live = await liveChecks(TOOLS, contract, tasks);
  for (const t of TOOLS) Object.assign(results[t], { output: live.perTool[t].skipped ? { ok: null, reasons: [live.perTool[t].skipped] } : live.perTool[t].output, communicate: live.perTool[t].skipped ? { ok: null, reasons: [live.perTool[t].skipped] } : live.perTool[t].communicate, samples: live.perTool[t].samples, skipped: live.perTool[t].skipped });
}
const addonAfter = addonStamp();

// ---------- Acceptance C1-C5 ----------
let quotas = {};
try { quotas = JSON.parse(fs.readFileSync(QUOTAS_FILE, 'utf8')); } catch {}
const published = Object.fromEntries(ALL_TOOLS.map(t => [t, { description: pub[t].description, inputSchema: pub[t].inputSchema }]));
const zeroText = zeroBasedTexts(published);
const budget = surfaceBudget(defaultSurface, defaultInstructionsBytes, { quotas });
const acceptance = Object.fromEntries(CHECKS.map(c => [c, { strict: STRICT.has(c), perTool: {} }]));
for (const t of TOOLS) {
  const lv = live?.perTool[t];
  const liveAcc = lv && !lv.skipped ? lv.acceptance : null;
  const set = (c, reasons, ran = true) => { acceptance[c].perTool[t] = { ok: ran ? reasons.length === 0 : null, reasons: ran ? reasons : ['not run (live off or tool skipped)'] }; };
  set('C1', liveAcc?.C1 ?? [], !!liveAcc);
  set('C2', liveAcc?.C2 ?? [], !!liveAcc);
  set('C3', [...zeroText.filter(z => z.startsWith(`${t}.`)), ...(liveAcc?.C3 ?? [])]);
  set('C4', liveAcc?.C4 ?? [], !!liveAcc);
  set('C5', descriptionLint(t, pub[t].description, ALL_TOOLS, x => publishedFields[x]));
}
acceptance.C5.surface = { tools: Object.keys(defaultSurface).length, toolsList: Object.values(defaultSurface).reduce((a, b) => a + b, 0) + Math.max(0, Object.keys(defaultSurface).length - 1) + 2, instructions: defaultInstructionsBytes, total: budget.total, perTool: defaultSurface, reasons: budget.reasons };

const STEPS = ['description', 'schema', 'input', 'execute', 'output', 'communicate'];
const cell = r => (!r ? '·' : r.ok === null ? '—' : r.ok ? '✓' : '✗');
const summary = Object.fromEntries(STEPS.map(s => [s, { pass: TOOLS.filter(t => results[t][s]?.ok === true).length, fail: TOOLS.filter(t => results[t][s]?.ok === false).length, na: TOOLS.filter(t => results[t][s] && results[t][s].ok === null).length }]));
const payload = {
  at: new Date().toISOString(), startedAt, addon: { before: addonBefore, after: addonAfter, changedDuringRun: addonBefore !== addonAfter },
  options: { live: LIVE, github: GITHUB, clasify: PAID, localPages: LOCAL_PAGES, remotePages: REMOTE_PAGES, leadReplays: LEAD_REPLAYS },
  bytes: { toolsList: toolsListBytes, instructions: instructionsBytes, combined: toolsListBytes + instructionsBytes, perTool: Object.fromEntries(TOOLS.map(t => [t, pub[t].bytes])) },
  githubCalls: live?.ghCalls ?? 0, summary, acceptance, probes: Object.fromEntries(TOOLS.map(t => [t, live?.perTool[t]?.probes ?? null])), crossToolFields: schema.table, tools: results,
};

function markdown(p) {
  const L = [];
  L.push(`# Tool chain check${label ? `: ${label}` : ''}`, '');
  L.push(`Generated by \`octocode-local-testing/harness/chain-check.mjs\` at ${p.at}. Native addon ${p.addon.before}${p.addon.changedDuringRun ? ` → ${p.addon.after} (CHANGED during the run)` : ''}. Live: ${p.options.live ? `yes (GitHub ${p.options.github ? 'on' : 'off'}, clasify ${p.options.clasify ? 'on' : 'off (paid)'}, ${p.githubCalls} remote calls)` : 'no'}.`, '');
  L.push(`tools/list ${p.bytes.toolsList} B + instructions ${p.bytes.instructions} B = ${p.bytes.combined} B.`, '');
  L.push('## Matrix', '', '✓ pass · ✗ fail · — not run (reason in the details)', '');
  L.push(`| Tool | ${STEPS.map((s, i) => `${i + 1} ${s[0].toUpperCase()}${s.slice(1)}`).join(' | ')} |`, `|---|${STEPS.map(() => ':-:').join('|')}|`);
  for (const t of TOOLS) L.push(`| ${t}${pub[t].cliOnly ? ' (CLI)' : ''} | ${STEPS.map(s => cell(p.tools[t][s])).join(' | ')} |`);
  L.push(`| **pass / fail** | ${STEPS.map(s => `${p.summary[s].pass} / ${p.summary[s].fail}${p.summary[s].na ? ` (${p.summary[s].na} —)` : ''}`).join(' | ')} |`, '');
  L.push('## Reasons', '', '| Tool | Step | Reasons |', '|---|---|---|');
  for (const t of TOOLS) for (const s of STEPS) { const r = p.tools[t][s]; if (r && r.ok !== true && r.reasons?.length) L.push(`| ${t} | ${s} | ${r.reasons.map(x => String(x).replace(/\|/g, '\\|')).join('<br>')} |`); }
  L.push('', '## Step 2: same name across tools (published top-level fields)', '', '| Field | Tools | Schema | Behavior note | Decision | Status |', '|---|---|--:|---|---|---|');
  for (const r of p.crossToolFields) L.push(`| \`${r.field}\` | ${r.tools.length === Object.keys(pub).length ? `all ${r.tools.length}` : r.tools.join(', ')} | ${r.schemaVariants} | ${r.note.replace(/\|/g, '/')} | ${r.decision} | ${r.aligned} |`);
  L.push('', '## Step 3/4 detail (native source)', '', '| Tool | Entry query param | Tool-side validators | Module lines (prod files) | Shared refs (≤ target) |', '|---|---|---|--:|--:|');
  for (const t of TOOLS) { const i = p.tools[t].input; const x = p.tools[t].execute; L.push(`| ${t} | ${i.entries.map(e => `\`${e.fn}(${e.param.replace(/\|/g, '/')})\` ${e.kind}`).join('<br>') || '—'} | ${i.toolValidators.length} | ${x.moduleLines} | ${x.sharedRefs} (≤ ${x.refTarget}) |`); }
  if (p.options.live) {
    L.push('', '## Step 5/6 detail (live samples)', '', '| Tool | Samples (pages, exhausted) | Pages with next / missing "N more" | Terminal pages / stale | Max leads | Pagination keys | Natural next | Replays |', '|---|---|--:|--:|--:|---|---|---|');
    for (const t of TOOLS) {
      const r = p.tools[t];
      if (r.skipped) { L.push(`| ${t} | — ${r.skipped} | | | | | | |`); continue; }
      const c = r.communicate;
      const esc = v => String(v).replace(/\|/g, '\\|');
      L.push(`| ${t} | ${r.samples.map(s => `${s.id} ${s.pages}p${s.exhausted ? '' : '+'}${s.errors.length ? ' err' : ''}`).join(', ')} | ${c.pagesWithNext} / ${c.missingMore.length} | ${c.terminal} / ${c.staleTerminal.length} | ${c.hintsMax} | ${esc(r.output.paginationKeys.join('; '))} | ${esc(c.natural ?? '—')} | ${esc(c.replays.map(x => `${x.name}→${x.tool} ${x.ok === null ? `— ${x.detail}` : x.ok ? '✓' : '✗'}`).join(', ') || '—')} |`);
    }
  }
  L.push('', '## Acceptance (FIX-LIST B1 checks)', '', `Strict: ${[...STRICT].join(',') || 'none (report mode)'}. Default surface (beta off, ${p.acceptance.C5.surface.tools} tools): tools/list ${p.acceptance.C5.surface.toolsList} B + instructions ${p.acceptance.C5.surface.instructions} B = ${p.acceptance.C5.surface.total} B${p.acceptance.C5.surface.reasons.length ? ` (${p.acceptance.C5.surface.reasons.join('; ')})` : ''}.`, '');
  L.push(`| Tool | ${CHECKS.join(' | ')} |`, `|---|${CHECKS.map(() => ':-:').join('|')}|`);
  for (const t of TOOLS) L.push(`| ${t} | ${CHECKS.map(c => { const r = p.acceptance[c].perTool[t]; return r.ok === null ? '—' : r.ok ? '✓' : `✗ ${r.reasons.length}`; }).join(' | ')} |`);
  L.push(`| **fail** | ${CHECKS.map(c => TOOLS.filter(t => p.acceptance[c].perTool[t].ok === false).length).join(' | ')} |`, '');
  L.push('| Tool | Check | Findings (first 6) |', '|---|---|---|');
  const esc = v => String(v).replace(/\|/g, '\\|');
  for (const c of CHECKS) for (const t of TOOLS) { const r = p.acceptance[c].perTool[t]; if (r.ok === false) L.push(`| ${t} | ${c} | ${r.reasons.slice(0, 6).map(esc).join('<br>')}${r.reasons.length > 6 ? `<br>(+${r.reasons.length - 6})` : ''} |`); }
  return L.join('\n') + '\n';
}

const label = opt('write');
const md = markdown(payload);
if (label) {
  fs.mkdirSync(OUT, { recursive: true });
  fs.writeFileSync(path.join(OUT, `chain-check-${label}.json`), JSON.stringify(payload, null, 1));
  fs.writeFileSync(path.join(OUT, `chain-check-${label}.md`), md);
  console.error(`wrote ${path.relative(ROOT, OUT)}/chain-check-${label}.md and .json`);
}
process.stdout.write(flag('json') ? JSON.stringify(payload, null, 1) + '\n' : md);
// run-all lines: strict checks PASS/FAIL; report-mode misses print REPORT (not counted).
if (!flag('json')) {
  const lines = [];
  for (const c of CHECKS) for (const t of TOOLS) {
    const r = acceptance[c].perTool[t];
    if (r.ok === null) continue;
    if (r.ok) lines.push(`PASS chain ${c} ${t}`);
    else lines.push(`${acceptance[c].strict ? 'FAIL' : 'REPORT'} chain ${c} ${t}: ${r.reasons.length} finding(s); ${short(r.reasons[0], 180)}`);
  }
  if (acceptance.C5.surface.reasons.length) lines.push(`${acceptance.C5.strict ? 'FAIL' : 'REPORT'} chain C5 surface: ${acceptance.C5.surface.reasons.join('; ')}`);
  process.stdout.write(`\n${lines.join('\n')}\n`);
  if (lines.some(l => l.startsWith('FAIL'))) process.exitCode = 1;
}
