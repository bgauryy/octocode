import { describe, expect, it } from 'vitest';
import { oqlSchemaText, OQL_SCHEMA_DOC } from '../../src/oql/schemeText.js';
import { ACTIVE_TARGETS } from '../../src/oql/types.js';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { OqlValidationError } from '../../src/oql/diagnostics.js';

describe('parity gap #1: --scheme query.target lists every active target', () => {
  it('query.target text == ACTIVE_TARGETS', () => {
    const doc = JSON.parse(oqlSchemaText());
    for (const t of ACTIVE_TARGETS) {
      expect(doc.query.target).toContain(t);
    }
  });
  it('per-target params hints exist for V2 targets', () => {
    for (const t of [
      'semantics',
      'repositories',
      'packages',
      'pullRequests',
      'commits',
      'artifacts',
      'diff',
      'research',
    ]) {
      expect(
        (OQL_SCHEMA_DOC.params as Record<string, unknown>)[t]
      ).toBeTruthy();
    }
  });
});

describe('parity gap #2: unsupportedTarget repair names current active targets', () => {
  it('repair message lists active targets, not the old V1-only set', () => {
    try {
      normalizeQuery({
        target: 'fixes',
        from: { kind: 'local', path: '.' },
      } as never);
      throw new Error('should throw');
    } catch (err) {
      const msg =
        (err as OqlValidationError).diagnostics[0]?.repair?.message ?? '';
      expect(msg).toContain('repositories');
      expect(msg).toContain('semantics');
    }
  });
});
