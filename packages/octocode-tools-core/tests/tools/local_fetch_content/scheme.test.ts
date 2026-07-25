import { describe, expect, it } from 'vitest';

import { LocalFetchContentQuerySchema } from '../../../src/tools/local_fetch_content/scheme.js';

describe('localGetFileContent schema', () => {
  // The schema must NOT inject a minify default: the direct-tool executor parses
  // inputSchema before execution, so a schema default would erase "caller omitted
  // minify" and silently defeat the fullContent→none resolution done in
  // fetchContent (the same class of bug fixed for ghGetFileContent). The
  // effective default is resolved in execution, not here.
  it('does not inject a minify default (omitted stays undefined)', () => {
    const query = LocalFetchContentQuerySchema.parse({
      path: '/repo/src/index.ts',
      fullContent: true,
    });

    expect(query.minify).toBeUndefined();
  });

  it('preserves explicit minify none for exact reads', () => {
    const query = LocalFetchContentQuerySchema.parse({
      path: '/repo/src/index.ts',
      fullContent: true,
      minify: 'none',
    });

    expect(query.minify).toBe('none');
  });
});
