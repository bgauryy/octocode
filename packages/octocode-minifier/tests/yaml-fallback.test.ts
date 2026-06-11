// Tests for jsonToYamlString error-recovery paths (lines 97-103 in yaml source).
// Kept in a separate file without any vi.mock() so v8 coverage can track
// the error-handling code paths accurately.

import { describe, it, expect } from 'vitest';
import { jsonToYamlString } from '@octocodeai/octocode-minifier';

describe('jsonToYamlString — double-fallback (circular / non-serialisable)', () => {
  it('returns an error comment when both js-yaml AND JSON.stringify fail', () => {
    // Circular references cause js-yaml (noRefs:true) to recurse until stack
    // overflow, then JSON.stringify also throws → innermost catch fires.
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    const result = jsonToYamlString(circular);

    expect(typeof result).toBe('string');
    expect(result).toContain('# YAML conversion failed:');
    expect(result).toContain('# JSON conversion also failed:');
    expect(result).toContain('# Object: [Unconvertible]');
  });

  it('includes both error messages in the fallback comment', () => {
    const a: Record<string, unknown> = {};
    const b: Record<string, unknown> = { back: a };
    a.fwd = b;

    const result = jsonToYamlString(a);

    expect(result).toMatch(/# YAML conversion failed: .+/);
    expect(result).toMatch(/# JSON conversion also failed: .+/);
  });

  it('does not throw for circular structures', () => {
    const cycle: Record<string, unknown> = { x: 1 };
    cycle.loop = cycle;

    expect(() => jsonToYamlString(cycle)).not.toThrow();
  });

  it('returns a string (never undefined) for circular structures', () => {
    const circ: Record<string, unknown> = {};
    circ.circ = circ;

    const result = jsonToYamlString(circ);
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });
});
