// astSearch match vs ast-grep CLI (and rg text approximation); astSearch symbols vs rg decl regex / ctags.
// Truth for match = ast-grep 0.45 CLI --json=stream (independent binary). Truth for symbols = ast-grep
// kind rules capturing the `name` field (independent of octocode's extractor), adjudicated in REPORT.
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPOS, sh, ocAll, pr, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
const run = (argv) => spawnSync(argv[0], argv.slice(1), { cwd: REPOS, encoding: 'utf8', maxBuffer: 1 << 29 }).stdout;
const sgJson = (args) => run(['ast-grep', ...args, '--json=stream']).split('\n').filter(Boolean).map(l => JSON.parse(l));

function ocMatchKeys(all) {
  const k = [];
  for (const p of all.parts) { if (!p) continue;
    for (const r of p.results || []) for (const f of r.data?.files || []) for (const m of f.matches || []) k.push(`${path.relative(REPOS, path.resolve(p.base || '', f.path))}:${m.line}`); }
  return k;
}
const out = [];
const fmt = (c) => c.precision !== undefined ? `P${c.precision}/R${c.recall} (${c.found}/${c.truth})` : JSON.stringify(c).slice(0, 60);
function push(r) { out.push(r); console.log(`${r.id.padEnd(22)} shell ${r.shell.calls}x ${String(r.shell.chars).padStart(7)}c ${String(r.shell.ms).padStart(5)}ms ${fmt(r.shell.correct).padEnd(26)} | oc ${r.octocode.calls}x ${String(r.octocode.chars).padStart(7)}c ${String(r.octocode.ms).padStart(5)}ms ${fmt(r.octocode.correct).padEnd(26)} ${r.rg ? '| rg ' + r.rg.chars + 'c ' + fmt(r.rg.correct) : ''}`); }

const MATCH = [
  ['rust', 'rust/tokio/src/sync', 'rust', '$X.unwrap()', `rg -n -F '.unwrap()' -t rust rust/tokio/src/sync`],
  ['ts', 'tsx/packages/element/src', 'typescript', 'new Error($MSG)', `rg -n 'new Error\\(' -g '*.ts' tsx/packages/element/src`],
  ['go', 'go/tsdb', 'go', 'errors.New($S)', `rg -n -F 'errors.New(' -t go go/tsdb`],
  ['python', 'python/django/contrib', 'python', '$M.objects.filter($$$A)', `rg -n '\\w+\\.objects\\.filter\\(' -t py python/django/contrib`],
  ['java', 'java/guava/src/com/google/common/base', 'java', 'checkNotNull($X)', `rg -n -w 'checkNotNull\\([^,()]*\\)' -t java java/guava/src/com/google/common/base`],
  ['c', 'c/src', 'c', 'zfree($X)', `rg -n '\\bzfree\\(' -g '*.c' c/src`],
  ['cpp', 'cpp/include', 'cpp', 'JSON_THROW($E)', `rg -n -F 'JSON_THROW(' cpp/include`],
];
for (const [lang, dir, sgLang, pattern, rgCmd] of MATCH) {
  // Go/C bare-call patterns misparse in the ast-grep CLI (type_conversion / macro_type_specifier), so truth
  // uses a context+selector rule; the shell side pays for the failed first try plus the fixed rule.
  const ctx = { go: `func f() { ${pattern} }`, c: `void f() { ${pattern}; }` }[lang];
  const rule = ctx ? `id: e\nlanguage: ${sgLang}\nrule:\n  pattern:\n    context: ${JSON.stringify(ctx)}\n    selector: call_expression` : null;
  const truthRows = rule ? sgJson(['scan', '--inline-rules', rule, dir]) : sgJson(['run', '-p', pattern, '-l', sgLang, dir]);
  const truth = [...new Set(truthRows.map(m => `${m.file}:${m.range.start.line + 1}`))];
  const cmd = `ast-grep run -p '${pattern}' -l ${sgLang} ${dir}`;
  let s = sh(cmd, { cwd: REPOS });
  let shellCalls = 1;
  if (rule) {
    const cmd2 = `ast-grep scan --inline-rules $'${rule.replace(/\n/g, '\\n').replace(/'/g, "\\'")}' ${dir} --json=stream | jq -r '"\\(.file):\\(.range.start.line+1): \\(.lines)"'`;
    const s2 = sh(cmd2, { cwd: REPOS });
    s = { ...s2, cmd: cmd + '  ⟶ 0 hits, then: ' + cmd2, chars: s.chars + s2.chars, ms: s.ms + s2.ms, stdout: s2.stdout };
    shellCalls = 2;
  }
  const shellKeys = [...new Set(s.stdout.split('\n').map(l => l.match(/^(.+?):(\d+):/)).filter(Boolean).map(m => `${m[1]}:${m[2]}`))];
  const q = { operation: 'match', path: A(dir), langType: sgLang, pattern, pageSize: 100, maxMatchesPerFile: 1000 };
  const o = ocAll('astSearch', [q]);
  const rg = sh(rgCmd, { cwd: REPOS });
  const rgKeys = rg.stdout.split('\n').map(l => l.match(/^(.+?):(\d+):/)).filter(Boolean).map(m => `${m[1]}:${m[2]}`);
  push({ id: `match-${lang}`, lang, task: `Structural matches of \`${pattern}\` in ${dir}`,
    shell: { cmd: s.cmd || cmd, calls: shellCalls, chars: s.chars, ms: s.ms, correct: { ...pr(shellKeys, truth), note: 'plain ast-grep prints every line of a multi-line match; precision<1 here is rendering, not wrong matches' } },
    octocode: { query: q, calls: o.calls, chars: o.chars, ms: o.ms, correct: pr([...new Set(ocMatchKeys(o))], truth), steps: o.steps },
    rg: { cmd: rgCmd, chars: rg.chars, ms: rg.ms, correct: pr([...new Set(rgKeys)], truth) } });
}

// ---- symbols: function/method(/class) declarations in one file, compared as name multisets
const nameRule = (lang, kinds) => `id: s\nlanguage: ${lang}\nrule:\n  any:\n${kinds.map(k => `    - kind: ${k}\n      has:\n        field: ${k === 'function_definition' && (lang === 'c' || lang === 'cpp') ? 'declarator' : 'name'}\n        pattern: $N`).join('\n')}`;
const SYM = [
  ['go', 'go/tsdb/head.go', 'go', ['function_declaration', 'method_declaration'], ['function', 'method'], `rg -n '^func ' go/tsdb/head.go`, (l) => l.match(/^\d+:func\s+(?:\([^)]*\)\s*)?(\w+)/)?.[1]],
  ['rust', 'rust/tokio/src/runtime/blocking/pool.rs', 'rust', ['function_item'], ['function', 'method'], `rg -n '^\\s*(pub(\\([\\w:]+\\))? )?(const )?(async )?(unsafe )?fn \\w+' rust/tokio/src/runtime/blocking/pool.rs`, (l) => l.match(/fn (\w+)/)?.[1]],
  ['python', 'python/django/db/models/query.py', 'python', ['function_definition', 'class_definition'], ['function', 'method', 'class'], `rg -n '^\\s*(async )?(def|class) \\w+' python/django/db/models/query.py`, (l) => l.match(/(?:def|class) (\w+)/)?.[1]],
  ['java', 'java/guava/src/com/google/common/collect/Lists.java', 'java', ['method_declaration', 'constructor_declaration'], ['function', 'method', 'constructor'], `rg -n '^\\s*(public|protected|private|static|final|abstract|synchronized|native|default|\\s)*(<[^>]+>\\s+)?[\\w.<>\\[\\],?@ ]+\\s+\\w+\\s*\\([^;]*$' java/guava/src/com/google/common/collect/Lists.java`, (l) => l.match(/(\w+)\s*\([^(]*$/)?.[1]],
  ['ts', 'tsx/packages/element/src/newElement.ts', 'typescript', ['function_declaration'], ['function', 'method'], `rg -n '^\\s*(export )?(async )?(function \\w+|const \\w+ = (async )?(<[^>]*>)?\\()' tsx/packages/element/src/newElement.ts`, (l) => l.match(/(?:function|const) (\w+)/)?.[1]],
  ['c', 'c/src/zmalloc.c', 'c', ['function_definition'], ['function'], `ctags -x c/src/zmalloc.c`, (l) => { const m = l.match(/^(\w+)\s+(\d+)\s+\S+\s+(.*)$/); return m && /\(/.test(m[3]) && !/^#\s*define/.test(m[3]) ? m[1] : null; }],
];
const tsArrowRule = `id: s\nlanguage: typescript\nrule:\n  any:\n    - kind: function_declaration\n      has:\n        field: name\n        pattern: $N\n    - kind: variable_declarator\n      all:\n        - has:\n            field: name\n            pattern: $N\n        - has:\n            field: value\n            kind: arrow_function`;
const cRule = `id: s\nlanguage: c\nrule:\n  kind: function_declarator\n  inside:\n    kind: function_definition\n    stopBy: end\n  not:\n    inside:\n      kind: parameter_declaration\n      stopBy: end\n  has:\n    field: declarator\n    kind: identifier\n    pattern: $N`;
for (const [lang, file, sgLang, kinds, ocKinds, shellCmd, nameOf] of SYM) {
  const rule = lang === 'ts' ? tsArrowRule : lang === 'c' ? cRule : nameRule(sgLang, kinds);
  let rows = sgJson(['scan', '--inline-rules', rule, file]);
  if (lang === 'c') rows = rows.filter(r => r.metaVariables?.single?.N); // direct declarator of a definition
  const truth = rows.map(r => r.metaVariables?.single?.N?.text).filter(Boolean);
  // tree-sitter leaves macro bodies as token trees; these fns inside cfg_*! { } blocks were verified by reading pool.rs:47,257,398,533,537
  if (lang === 'rust') truth.push('queue_depth', 'spawn_mandatory_blocking', 'spawn_mandatory_blocking', 'num_idle_threads', 'queue_depth');
  truth.sort();
  const s = sh(shellCmd, { cwd: REPOS });
  const shellNames = s.stdout.split('\n').filter(Boolean).map(nameOf).filter(Boolean).sort();
  const q = { operation: 'symbols', path: A(file), pageSize: 500 };
  const o = ocAll('astSearch', [q]);
  const decls = o.parts.flatMap(p => p?.results?.flatMap(r => r.data?.declarations || []) || []);
  const ocNames = decls.filter(d => ocKinds.includes(d.kind)).map(d => d.name).sort();
  const ms = (got) => { const T = [...truth], extra = []; for (const g of got) { const i = T.indexOf(g); if (i >= 0) T.splice(i, 1); else extra.push(g); } const tp = got.length - extra.length;
    return { found: got.length, truth: truth.length, tp, precision: +(got.length ? tp / got.length : 0).toFixed(3), recall: +(tp / truth.length).toFixed(3), missing: T.slice(0, 12), extra: extra.slice(0, 12) }; };
  push({ id: `symbols-${lang}`, lang, task: `Function/method declarations in ${path.basename(file)}`,
    shell: { cmd: shellCmd, calls: 1, chars: s.chars, ms: s.ms, correct: ms(shellNames) },
    octocode: { query: q, calls: o.calls, chars: o.chars, ms: o.ms, correct: ms(ocNames), totalDeclarations: decls.length, kindsCounted: ocKinds, steps: o.steps },
    truthRule: rule });
}
saveRaw('ast', out);
