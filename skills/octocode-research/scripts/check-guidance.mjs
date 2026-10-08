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
    'Checks documented JSON tool examples against live query schemas. No tool execution or writes.',
    '--examples  validate examples (the default; combine with --self-test to run both)',
    '            against the live query schema from `octocode schema <tool>`.',
    '            CLI lookup: $OCTOCODE_CLI (a .js file or binary), the monorepo build, then `octocode` on PATH.',
    '            A failing selected CLI is reported without switching to a different installed version.',
    '            An unavailable CLI is reported as an incomplete check and exits nonzero.',
  ].join('\n'));
  process.exit(0);
}
if (args.some(arg => !['--json', '--self-test', '--examples'].includes(arg))) {
  console.error('Unknown option; use --help.');
  process.exit(2);
}

const checks = [];
const selfChecks = [];

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
  let cli = process.env.OCTOCODE_CLI;
  for (let dir = root; !cli && dirname(dir) !== dir; dir = dirname(dir)) {
    const built = join(dir, 'packages/octocode/out/octocode.js');
    if (existsSync(built)) cli = built;
  }
  cli ||= 'octocode';
  const [cmd, pre] = cli.endsWith('.js') ? [process.execPath, [cli]] : [cli, []];
  try {
    execFileSync(cmd, [...pre, 'schema'], { stdio: 'pipe', timeout: 20000 });
    return (tool) => JSON.parse(execFileSync(cmd, [...pre, 'schema', tool, '--view', 'query'], {
      stdio: 'pipe', timeout: 20000, env: { ...process.env, OCTOCODE_BETA: '1' },
    }).toString()).querySchema;
  } catch (error) {
    // A failing selected build must not be replaced by another version on PATH.
    resolveCli.lastError = `${cli}: ${commandFailure(error)}`;
  }
  return undefined;
}

function commandFailure(error) {
  const output = [error?.stderr, error?.stdout].map(value => String(value ?? '').trim()).filter(Boolean);
  return output.length ? output.join('\n') : String(error?.message || error);
}

let exampleNote;
if (!args.includes('--self-test') || args.includes('--examples')) {
  const schemaFor = resolveCli();
  if (!schemaFor) {
    checks.push({ name: 'live schema available', pass: false });
    exampleNote = resolveCli.lastError
      ? `INCOMPLETE examples: octocode CLI unusable (${resolveCli.lastError}); schema validation not run.`
      : 'INCOMPLETE examples: no octocode CLI found (set OCTOCODE_CLI, build packages/octocode, or put `octocode` on PATH); schema validation not run.';
  } else {
    const schemas = new Map();
    const examples = extractExamples();
    if (!examples.length) checks.push({ name: 'documented tool examples found', pass: false });
    for (const ex of examples) {
      let errors;
      if (ex.parseError) errors = ['example JSON does not parse'];
      else {
        if (!schemas.has(ex.tool)) {
          try { schemas.set(ex.tool, { schema: schemaFor(ex.tool) }); }
          catch (error) { schemas.set(ex.tool, { error: commandFailure(error) }); }
        }
        const result = schemas.get(ex.tool);
        errors = result.schema ? validateSchema(result.schema, ex.query, result.schema)
          : [`schema unavailable for ${ex.tool}: ${result.error || 'querySchema missing from response'}`];
      }
      checks.push({ name: `example ${ex.tool} valid against live schema${errors.length ? `: ${errors.join('; ')}` : ''}`, file: `${ex.file}:${ex.line}`, pass: errors.length === 0 });
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
  selfChecks.push({ name: 'CLI failure preserves structured stdout with empty stderr',
    pass: commandFailure({ stderr: Buffer.from(''), stdout: Buffer.from('{"error":"contract drift"}') }) === '{"error":"contract drift"}' });
  selfChecks.push({ name: 'CLI failure preserves both output streams',
    pass: commandFailure({ stderr: Buffer.from('diagnostic'), stdout: Buffer.from('details') }) === 'diagnostic\ndetails' });
}
const all = [...checks, ...selfChecks];
const failed = all.filter(check => !check.pass);
const report = { pass: failed.length === 0, passed: all.length - failed.length, total: all.length, ...(exampleNote ? { examples: exampleNote } : {}), checks: all };
if (args.includes('--json')) console.log(JSON.stringify(report, null, 2));
else {
  console.log(`${report.pass ? 'PASS' : 'FAIL'} research-examples ${report.passed}/${report.total}`);
  if (exampleNote) console.log(`  ${exampleNote}`);
  for (const check of failed) console.log(`  FAIL ${check.name}${check.file ? ` (${check.file})` : ''}`);
}
process.exitCode = report.pass ? 0 : 1;
