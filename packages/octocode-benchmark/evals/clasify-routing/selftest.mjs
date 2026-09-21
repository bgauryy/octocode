import assert from 'node:assert/strict';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { cases, categories } from './cases.mjs';

// Root mode accepts an immutable pre-split benchmark snapshot; default resolves
// the live shared catalog. This selftest checks corpus wellformedness and the
// contract-integrity precondition only — it does NOT measure agent selection.
const root = process.argv[2] ? path.resolve(process.argv[2]) : undefined;
const schema = root
  ? await import(pathToFileURL(path.join(root, 'baseline/core-dist/schema.js')))
  : await import('@octocodeai/config/schema');
const specs =
  schema.DIRECT_TOOL_DEFINITIONS ?? (root ? schema.DIRECT_TOOL_SPECIFICATIONS : undefined);
if (!Array.isArray(specs)) throw Error('Tool definitions are unavailable');

let assertions = 0;
const names = specs.map((tool) => tool.name);

// Contract-integrity precondition: hard cutover to clasify.
assert.ok(names.includes('clasify'), 'catalog must advertise clasify');
assertions++;
assert.ok(!names.includes('semanticAssess'), 'catalog must not advertise semanticAssess');
assertions++;

// Corpus wellformedness.
const seen = new Set();
for (const test of cases) {
  assert.ok(test.id && !seen.has(test.id), `duplicate or missing case id: ${test.id}`);
  seen.add(test.id);
  assert.ok(categories.includes(test.category), `${test.id}: unknown category`);
  assert.ok(typeof test.prompt === 'string' && test.prompt.length > 0, `${test.id}: empty prompt`);
  assert.ok(test.expected && typeof test.expected === 'object', `${test.id}: missing expected`);
  // The grader-only reference must never leak the target tool name into a prompt.
  assert.ok(!/clasify|semanticAssess/i.test(test.prompt), `${test.id}: prompt leaks the tool name`);
  assertions += 4;

  if (test.category === 'positive') {
    assert.equal(test.expected.tool, 'clasify', `${test.id}: positive must expect clasify`);
    assert.ok(['noul', 'choice', 'score'].includes(test.expected.judgment), `${test.id}: bad judgment`);
    assertions += 2;
  }
  if (test.category === 'negative') {
    assert.equal(test.expected.notTool, 'clasify', `${test.id}: negative must forbid clasify`);
    assert.notEqual(test.expected.tool, 'clasify', `${test.id}: negative routed to clasify`);
    assertions += 2;
  }
  if (test.category === 'availability') {
    assert.equal(typeof test.keyPresent, 'boolean', `${test.id}: availability needs keyPresent`);
    if (test.keyPresent === false) {
      assert.deepEqual(test.disabledTools, ['clasify'], `${test.id}: keyless case must disable clasify`);
    }
    assertions += 2;
  }
}

// Every category is represented so the eval covers routing, non-routing, and gating.
for (const category of categories) {
  assert.ok(cases.some((test) => test.category === category), `no cases for category ${category}`);
  assertions++;
}

console.log(JSON.stringify({ status: 'PASS', cases: cases.length, assertions }));
