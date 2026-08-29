import { describe, expect, it } from 'vitest';

import { countBy } from '../../../src/tools/lsp/semantic_content/semanticFileOps/documentSymbols.js';

describe('document symbol kind counts', () => {
  it('counts Object prototype key names numerically', () => {
    const counts = countBy(
      ['constructor', 'constructor', 'toString'],
      value => value
    );

    expect(counts).toEqual({ constructor: 2, toString: 1 });
  });
});
