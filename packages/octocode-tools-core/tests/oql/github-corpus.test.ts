import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { OqlValidationError } from '../../src/oql/diagnostics.js';

/**
 * Contract §source-and-scope: "GitHub content, structure, and materialization
 * require a concrete repository." A provider-wide ({kind:"github"}) or
 * owner-only source is valid only for provider-search targets (code,
 * repositories), not for content/structure reads of a specific tree.
 */
function expectInvalid(input: unknown): OqlValidationError {
  try {
    normalizeQuery(input as never);
  } catch (err) {
    expect(err).toBeInstanceOf(OqlValidationError);
    return err as OqlValidationError;
  }
  throw new Error('expected normalizeQuery to throw OqlValidationError');
}

describe('OQL github corpus: content/structure require a concrete repository', () => {
  it('content + github owner-only -> invalidQuery', () => {
    const err = expectInvalid({
      target: 'content',
      from: { kind: 'github', owner: 'facebook' },
      fetch: { content: { range: { startLine: 1, endLine: 5 } } },
    });
    expect(err.diagnostics[0]?.code).toBe('invalidQuery');
  });

  it('content + provider-wide github -> invalidQuery', () => {
    const err = expectInvalid({
      target: 'content',
      from: { kind: 'github' },
      fetch: { content: { range: { startLine: 1, endLine: 5 } } },
    });
    expect(err.diagnostics[0]?.code).toBe('invalidQuery');
  });

  it('structure + provider-wide github -> invalidQuery', () => {
    const err = expectInvalid({
      target: 'structure',
      from: { kind: 'github' },
      fetch: { tree: { maxDepth: 1 } },
    });
    expect(err.diagnostics[0]?.code).toBe('invalidQuery');
  });

  it('structure + owner-only github -> invalidQuery', () => {
    const err = expectInvalid({
      target: 'structure',
      from: { kind: 'github', owner: 'facebook' },
      fetch: { tree: { maxDepth: 1 } },
    });
    expect(err.diagnostics[0]?.code).toBe('invalidQuery');
  });

  it('content + concrete repo is valid', () => {
    const n = normalizeQuery({
      target: 'content',
      from: { kind: 'github', repo: 'facebook/react' },
      scope: { path: 'README.md' },
      fetch: { content: { range: { startLine: 1, endLine: 5 } } },
    } as never);
    expect(n.from).toEqual({ kind: 'github', repo: 'facebook/react' });
  });

  it('structure + concrete repo is valid', () => {
    const n = normalizeQuery({
      target: 'structure',
      from: { kind: 'github', repo: 'facebook/react' },
      fetch: { tree: { maxDepth: 1 } },
    } as never);
    expect(n.target).toBe('structure');
  });

  it('code + provider-wide github stays valid (provider search across GitHub)', () => {
    const n = normalizeQuery({
      target: 'code',
      from: { kind: 'github' },
      where: { kind: 'text', value: 'useEffect' },
    } as never);
    expect(n.from).toEqual({ kind: 'github' });
  });

  it('code + owner-only github stays valid (owner-scoped provider search)', () => {
    const n = normalizeQuery({
      target: 'code',
      from: { kind: 'github', owner: 'facebook' },
      where: { kind: 'text', value: 'useEffect' },
    } as never);
    expect(n.from).toEqual({ kind: 'github', owner: 'facebook' });
  });
});
