#!/usr/bin/env node
/** Offline regression gate for documented tool contracts, not an agent benchmark. */
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (args.includes('--help')) {
  console.log([
    'Usage: node scripts/check-guidance.mjs [--json] [--self-test] [--examples]',
    'Checks local/external routing, completeness, and TDD guidance. No network or writes.',
    '--examples  also validate every JSON tool-call example in SKILL.md, README.md, and references/*.md',
    '            against the live query schema from `octocode scheme <tool> --compact`.',
    '            CLI lookup: $OCTOCODE_CLI (a .js file or binary), the monorepo build, then `octocode` on PATH.',
    '            Without a CLI the example check is skipped with a clear message (exit unaffected).',
  ].join('\n'));
  process.exit(0);
}
if (args.some(arg => !['--json', '--self-test', '--examples'].includes(arg))) {
  console.error('Unknown option; use --help.');
  process.exit(2);
}

const cases = [
  { name: 'known anchors skip discovery', file: 'references/workflow-local.md',
    required: [/known (?:file|path|anchor)/i, /skip.*(?:discovery|orientation)/i] },
  { name: 'reachability verifies explicit or inferred roots', file: 'references/workflow-local.md',
    required: [/reachability[^\n]*optional[^\n]*entrypoints/i, /entrypointsResolved/, /unclassified/],
    forbidden: [/reachability[^\n]*required[^\n]*entrypoints/i] },
  { name: 'AST limits and rewrites are evidence', file: 'references/workflow-local.md',
    required: [/structural\.query\.rewritten/, /terminalLimit/, /(?:incomplete|partial)[^\n]*absence/i] },
  { name: 'LSP needs anchors and capability checks', file: 'references/workflow-local.md',
    required: [/uri[^\n]*symbolName[^\n]*lineHint/, /includeDeclaration:false/, /warmup/, /capabilit/i] },
  { name: 'graph coverage has independent diagnostics', file: 'references/workflow-local.md',
    required: [/diagnosticPage/, /unresolved[^\n]*CommonJS/, /rustWorkspace/, /syntactic/] },
  { name: 'artifact intent and version provenance', file: 'references/workflow-external.md',
    required: [/packageName[^\n]*exact/i, /keywords[^\n]*discovery/i, /(?:version|release)[^\n]*(?:gitHead|tag|commit)/i] },
  { name: 'GitHub indexed search has explicit boundaries', file: 'references/workflow-external.md',
    required: [/code[^\n]*default branch/i, /1,000/, /(?:incomplete|partial)/i] },
  { name: 'file refs never silently substitute', file: 'references/workflow-external.md',
    required: [/ghGetFileContent[^\n]*explicit[^\n]*branch/i, /404[^\n]*(?:path|ref)/i],
    forbidden: [/fallback branch changes what was researched/i] },
  { name: 'history operations keep distinct identities', file: 'references/workflow-external.md',
    required: [/pullRequest[^\n]*issue[^\n]*number/, /commit[^\n]*ref/, /compare[^\n]*base[^\n]*head/, /omit[^\n]*keywords[^\n]*(?:path|branch)/i] },
  { name: 'materialization respects scoped completeness and storage', file: 'references/workflow-combination.md',
    required: [/complete[^\n]*(?:relative|requested scope)/i, /OCTOCODE_STORAGE_MODE/, /ENABLE_CLONE/, /shallow[^\n]*history/i],
    forbidden: [/complete:false/, /3rd\+|third read|3\+ remote reads/i] },
  { name: 'transport and continuation semantics', file: 'references/octocode.md',
    required: [/responsePagination/, /nested/, /hasMore[^\n]*false/, /status[^\n]*error/],
    forbidden: [/\$OCTO cache fetch/, /only `clone` and `cache`/, /10 tools are enabled by default/] },
  { name: 'portable CLI invocation', file: 'references/octocode.md',
    required: [/npx -y octocode/, /node packages\/octocode\/out\/octocode\.js/],
    forbidden: [/\$OCTO /] },
  { name: 'TDD and no compatibility scaffolding', file: 'references/workflow-change.md',
    required: [/RED[^\n]*GREEN[^\n]*REFACTOR/, /(?:fail|failing)[^\n]*before[^\n]*(?:patch|edit|implementation)/i, /(?:no|avoid)[^\n]*compatibility[^\n]*(?:unless|without)/i, /rebuild[^\n]*(?:CLI|MCP)/i] },
  { name: 'authorization persists and budgets do not abandon work', file: 'SKILL.md',
    required: [/authorization[^\n]*(?:persists|carry|already)/i, /checkpoint[^\n]*(?:budget|time)|budget[^\n]*checkpoint/i],
    forbidden: [/Ask before public\/broad contracts/, /third unrelated search space/] },
  { name: 'conditional semantic crossroad is executable and evidence-bound', file: 'SKILL.md',
    required: [
      /MODEL[^\n]*SEMANTIC\?[^\n]*SEARCH\/READ/,
      /SEMANTIC\?[^\n]*conditional[^\n]*(?:never|not)[^\n]*mandatory/i,
      /explicit classification request/,
      /before the host reads a large known file/,
      /saved scrape text, browser snapshots/,
      /flat `questions:/,
      /unread `context:\{tool,query\}`/,
      /Skip literals, small exact reads/,
      /No automatic Scout → Judge chain/,
      /Hints do not establish source facts or global absence/,
      /verification reads and extra turns/,
      /If unavailable, use targeted direct reads/,
    ],
    forbidden: [/No current research workflow meets both gates/, /questions:\[\{id,question\}\]/, /context --compact/] },
  { name: 'primary sources and untrusted content', file: 'references/workflow-external.md',
    required: [/primary[^\n]*(?:documentation|docs)/i, /untrusted[^\n]*(?:instructions|data)/i] },
  { name: 'one owner for adaptive routing', file: 'references/workflows.md',
    required: [/surface[^\n]*task/i, /skip[^\n]*(?:irrelevant|redundant|known)/i],
    forbidden: [/take exactly one/, /routes don't nest/, /graph for file topology → LSP/] },
];

const corpus = new Map();
for (const item of cases) {
  if (!corpus.has(item.file)) corpus.set(item.file, readFileSync(resolve(root, item.file), 'utf8'));
}
const accepts = (item, source) =>
  item.required.every(pattern => pattern.test(source)) &&
  (item.forbidden ?? []).every(pattern => !pattern.test(source));
const checks = cases.map(item => ({ name: item.name, file: item.file, pass: accepts(item, corpus.get(item.file)) }));
const selfChecks = args.includes('--self-test')
  ? cases.map(item => ({ name: `${item.name}: missing guidance rejected`, pass: !accepts(item, '') }))
  : [];
if (args.includes('--self-test')) {
  const semantic = cases.find(item => item.name.startsWith('conditional semantic crossroad'));
  const source = corpus.get(semantic.file);
  for (const [name, removed] of [
    ['unread routing', 'before the host reads a large known file'],
    ['artifact workflow', 'saved scrape text, browser snapshots'],
    ['flat contract', 'flat `questions:'],
    ['unread context', 'unread `context:{tool,query}`'],
    ['exact-check bypass', 'Skip literals, small exact reads'],
    ['proof boundary', 'Hints do not establish source facts or global absence'],
    ['complete cost accounting', 'verification reads and extra turns'],
  ]) {
    const changed = source.replace(removed, '');
    selfChecks.push({ name: `clasify: missing ${name} rejected`, pass: changed !== source && !accepts(semantic, changed) });
  }
}

// ---- Example validation against the live tool schema -------------------------------------------
// A dependency-free JSON Schema subset covering every keyword the live query schemas use.
function validateSchema(schema, value, root, path = '$', errors = []) {
  if (schema === true || schema == null) return errors;
  if (schema === false) { errors.push(`${path}: not allowed`); return errors; }
  if (schema.$ref) {
    const target = schema.$ref.replace(/^#\//, '').split('/').reduce((node, key) => node?.[key], root);
    if (!target) errors.push(`${path}: unresolved ${schema.$ref}`);
    else validateSchema(target, value, root, path, errors);
  }
  const typeOf = v => (v === null ? 'null' : Array.isArray(v) ? 'array' : Number.isInteger(v) ? 'integer' : typeof v);
  if (schema.type) {
    const types = [].concat(schema.type);
    const actual = typeOf(value);
    if (!types.includes(actual) && !(actual === 'integer' && types.includes('number'))) {
      errors.push(`${path}: expected ${types.join('|')}, got ${actual}`);
      return errors;
    }
  }
  if ('const' in schema && JSON.stringify(schema.const) !== JSON.stringify(value)) errors.push(`${path}: expected ${JSON.stringify(schema.const)}`);
  if (schema.enum && !schema.enum.some(e => JSON.stringify(e) === JSON.stringify(value))) errors.push(`${path}: ${JSON.stringify(value)} not in ${JSON.stringify(schema.enum)}`);
  if (typeof value === 'string') {
    if (schema.minLength != null && value.length < schema.minLength) errors.push(`${path}: shorter than ${schema.minLength}`);
    if (schema.maxLength != null && value.length > schema.maxLength) errors.push(`${path}: longer than ${schema.maxLength}`);
    if (schema.pattern && !new RegExp(schema.pattern, 'u').test(value)) errors.push(`${path}: does not match ${schema.pattern}`);
  }
  if (typeof value === 'number') {
    if (schema.minimum != null && value < schema.minimum) errors.push(`${path}: below ${schema.minimum}`);
    if (schema.maximum != null && value > schema.maximum) errors.push(`${path}: above ${schema.maximum}`);
  }
  if (Array.isArray(value)) {
    if (schema.minItems != null && value.length < schema.minItems) errors.push(`${path}: fewer than ${schema.minItems} items`);
    if (schema.maxItems != null && value.length > schema.maxItems) errors.push(`${path}: more than ${schema.maxItems} items`);
    if (schema.items) value.forEach((item, i) => validateSchema(schema.items, item, root, `${path}[${i}]`, errors));
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const keys = Object.keys(value);
    if (schema.minProperties != null && keys.length < schema.minProperties) errors.push(`${path}: fewer than ${schema.minProperties} properties`);
    if (schema.maxProperties != null && keys.length > schema.maxProperties) errors.push(`${path}: more than ${schema.maxProperties} properties`);
    for (const key of schema.required ?? []) if (!(key in value)) errors.push(`${path}: missing required ${key}`);
    for (const key of keys) {
      if (schema.propertyNames) validateSchema(schema.propertyNames, key, root, `${path}{${key}}`, errors);
      if (schema.properties && key in schema.properties) validateSchema(schema.properties[key], value[key], root, `${path}.${key}`, errors);
      else if (schema.additionalProperties === false) errors.push(`${path}: unknown property ${key}`);
      else if (schema.additionalProperties && typeof schema.additionalProperties === 'object') validateSchema(schema.additionalProperties, value[key], root, `${path}.${key}`, errors);
    }
  }
  if (schema.allOf) for (const sub of schema.allOf) validateSchema(sub, value, root, path, errors);
  const passing = subs => subs.filter(sub => validateSchema(sub, value, root, path, []).length === 0).length;
  if (schema.anyOf && passing(schema.anyOf) === 0) errors.push(`${path}: matches no anyOf branch`);
  if (schema.oneOf && passing(schema.oneOf) !== 1) errors.push(`${path}: must match exactly one oneOf branch`);
  if (schema.not && validateSchema(schema.not, value, root, path, []).length === 0) errors.push(`${path}: matches a forbidden schema`);
  return errors;
}

/** Tool-call examples: fenced JSON `{tool,query}` items (array, object, or one per line) and CLI `octocode <tool> '<json>'` lines. */
function extractExamples() {
  const files = ['SKILL.md', 'README.md', ...readdirSync(join(root, 'references')).filter(f => f.endsWith('.md')).map(f => `references/${f}`)]
    .filter(f => existsSync(join(root, f)));
  const examples = [];
  const rows = (tool, query) => (Array.isArray(query?.queries) ? query.queries : [query]).map(row => ({ tool, query: row }));
  for (const file of files) {
    const text = readFileSync(join(root, file), 'utf8');
    for (const block of text.matchAll(/```json\n([\s\S]*?)```/g)) {
      const line = text.slice(0, block.index).split('\n').length + 1;
      let parsed;
      try { parsed = [JSON.parse(block[1])]; } catch {
        parsed = block[1].split('\n').filter(l => l.trim()).map(l => { try { return JSON.parse(l); } catch { return undefined; } });
      }
      for (const item of parsed.flat()) {
        if (item && typeof item.tool === 'string' && item.query) examples.push(...rows(item.tool, item.query).map(r => ({ ...r, file, line })));
      }
    }
    for (const [i, lineText] of text.split('\n').entries()) {
      const cli = lineText.match(/octocode(?:\.js)?\s+([a-z][A-Za-z]+)\s+'(\{.*\})'/);
      if (!cli) continue;
      try { examples.push(...rows(cli[1], JSON.parse(cli[2])).map(r => ({ ...r, file, line: i + 1 }))); }
      catch { examples.push({ tool: cli[1], query: undefined, file, line: i + 1, parseError: true }); }
    }
  }
  return examples;
}

function resolveCli() {
  const candidates = [];
  if (process.env.OCTOCODE_CLI) candidates.push(process.env.OCTOCODE_CLI);
  for (let dir = root; dirname(dir) !== dir; dir = dirname(dir)) {
    const built = join(dir, 'packages/octocode/out/octocode.js');
    if (existsSync(built)) { candidates.push(built); break; }
  }
  candidates.push('octocode');
  for (const cli of candidates) {
    const [cmd, pre] = cli.endsWith('.js') ? [process.execPath, [cli]] : [cli, []];
    try {
      execFileSync(cmd, [...pre, 'scheme', '--compact'], { stdio: 'pipe', timeout: 20000 });
      return (tool) => JSON.parse(execFileSync(cmd, [...pre, 'scheme', tool, '--compact'], {
        stdio: 'pipe', timeout: 20000, env: { ...process.env, OCTOCODE_BETA: '1' },
      }).toString()).querySchema;
    } catch (error) {
      // Keep the reason: a present-but-failing CLI (e.g. contract drift) is not "missing".
      if (!(error && error.code === 'ENOENT')) resolveCli.lastError = `${cli}: ${String(error?.stderr || error?.message || error).trim().split('\n')[0]}`;
    }
  }
  return undefined;
}

let exampleNote;
if (args.includes('--examples')) {
  const schemaFor = resolveCli();
  if (!schemaFor) {
    exampleNote = resolveCli.lastError
      ? `SKIP examples: octocode CLI unusable (${resolveCli.lastError}); schema validation not run.`
      : 'SKIP examples: no octocode CLI found (set OCTOCODE_CLI, build packages/octocode, or put `octocode` on PATH); schema validation not run.';
  } else {
    const schemas = new Map();
    const examples = extractExamples();
    for (const ex of examples) {
      let errors;
      if (ex.parseError) errors = ['example JSON does not parse'];
      else {
        if (!schemas.has(ex.tool)) { try { schemas.set(ex.tool, schemaFor(ex.tool)); } catch { schemas.set(ex.tool, undefined); } }
        const schema = schemas.get(ex.tool);
        errors = schema ? validateSchema(schema, ex.query, schema) : [`unknown tool ${ex.tool} (no live schema)`];
      }
      checks.push({ name: `example ${ex.tool} valid against live schema${errors.length ? `: ${errors.slice(0, 3).join('; ')}` : ''}`, file: `${ex.file}:${ex.line}`, pass: errors.length === 0 });
    }
    exampleNote = `examples: ${examples.length} validated against live schemas`;
  }
}
if (args.includes('--self-test')) {
  const schema = { type: 'object', required: ['path'], additionalProperties: false, $defs: { Mode: { enum: ['a', 'b'] } },
    properties: { path: { type: 'string', pattern: '\\S' }, mode: { $ref: '#/$defs/Mode' }, n: { type: 'integer', minimum: 1 } } };
  for (const [name, value, ok] of [
    ['valid example accepted', { path: '/x', mode: 'a', n: 2 }, true],
    ['unknown property rejected', { path: '/x', operation: 'y' }, false],
    ['missing required rejected', { mode: 'a' }, false],
    ['bad enum via $ref rejected', { path: '/x', mode: 'c' }, false],
    ['below minimum rejected', { path: '/x', n: 0 }, false],
  ]) selfChecks.push({ name: `example validator: ${name}`, pass: (validateSchema(schema, value, schema).length === 0) === ok });
}
const all = [...checks, ...selfChecks];
const failed = all.filter(check => !check.pass);
const report = { pass: failed.length === 0, passed: all.length - failed.length, total: all.length, ...(exampleNote ? { examples: exampleNote } : {}), checks: all };
if (args.includes('--json')) console.log(JSON.stringify(report, null, 2));
else {
  console.log(`${report.pass ? 'PASS' : 'FAIL'} research-guidance ${report.passed}/${report.total}`);
  if (exampleNote) console.log(`  ${exampleNote}`);
  for (const check of failed) console.log(`  FAIL ${check.name}${check.file ? ` (${check.file})` : ''}`);
}
process.exitCode = report.pass ? 0 : 1;
