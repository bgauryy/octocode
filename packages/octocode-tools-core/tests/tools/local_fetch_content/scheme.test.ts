import { describe, expect, it } from 'vitest';

import { LocalFetchContentQuerySchema } from '../../../src/tools/local_fetch_content/scheme.js';

describe('localGetFileContent schema', () => {
  it('defaults omitted minify to standard', () => {
    const query = LocalFetchContentQuerySchema.parse({
      path: '/repo/src/index.ts',
      fullContent: true,
    });

    expect(query.minify).toBe('standard');
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
