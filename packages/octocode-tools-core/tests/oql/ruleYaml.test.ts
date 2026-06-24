import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { compileWhere } from '../../src/oql/adapters/compile.js';
import { structuralRuleToYaml } from '../../src/oql/adapters/ruleYaml.js';
import { runOqlSearch } from '../../src/oql/run.js';
import { isBatchEnvelope } from '../../src/oql/types.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OQL_SRC = path.resolve(here, '../../src/oql');

describe('structuralRuleToYaml', () => {
  it('serializes a pattern + nested not/inside rule to engine YAML', () => {
    const yaml = structuralRuleToYaml({
      pattern: 'await $X',
      not: { inside: { kind: 'try_statement', stopBy: 'end' } },
    });
    expect(yaml).toBe(
      [
        'rule:',
        '  pattern: "await $X"',
        '  not:',
        '    inside:',
        '      kind: "try_statement"',
        '      stopBy: end',
      ].join('\n')
    );
  });

  it('serializes all[] as a YAML sequence of rule maps', () => {
    const yaml = structuralRuleToYaml({
      all: [{ pattern: 'foo($X)' }, { kind: 'call_expression' }],
    });
    expect(yaml).toBe(
      [
        'rule:',
        '  all:',
        '    - pattern: "foo($X)"',
        '    - kind: "call_expression"',
      ].join('\n')
    );
  });
});

describe('OQL structural rule execution (regression: JSON rule must lower to YAML)', () => {
  const yamlRule = [
    'rule:',
    '  pattern: "await $X"',
    '  not:',
    '    inside:',
    '      kind: "try_statement"',
    '      stopBy: end',
  ].join('\n');

  it('runs a JSON structural rule end-to-end without a rust-conversion error', async () => {
    const result = await runOqlSearch({
      target: 'code',
      from: { kind: 'local', path: OQL_SRC },
      where: {
        kind: 'structural',
        lang: 'ts',
        rule: {
          pattern: 'await $X',
          not: { inside: { kind: 'try_statement', stopBy: 'end' } },
        },
      },
    });
    if (isBatchEnvelope(result)) throw new Error('expected single envelope');
    // The bug produced an invalidQuery (rust String conversion). Assert it's gone.
    expect(result.diagnostics.some(d => d.code === 'invalidQuery')).toBe(false);
    expect(result.evidence.kind).not.toBe('unsupported');
  });

  it('passes a grep-compatible YAML rule string through unchanged', () => {
    const compiled = compileWhere({
      kind: 'structural',
      lang: 'ts',
      rule: yamlRule,
    });
    expect(compiled.match).toMatchObject({
      mode: 'structural',
      langType: 'ts',
      rule: yamlRule,
    });
  });

  it('accepts a grep-compatible YAML rule string end-to-end', async () => {
    const result = await runOqlSearch({
      target: 'code',
      from: { kind: 'local', path: OQL_SRC },
      where: {
        kind: 'structural',
        lang: 'ts',
        rule: yamlRule,
      },
    });
    if (isBatchEnvelope(result)) throw new Error('expected single envelope');
    expect(result.diagnostics.some(d => d.code === 'invalidQuery')).toBe(false);
    expect(result.evidence.kind).not.toBe('unsupported');
  });
});
