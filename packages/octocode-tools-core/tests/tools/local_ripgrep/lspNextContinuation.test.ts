import { describe, expect, it } from 'vitest';

import {
  buildSearchResult,
  type LocalSearchEngine,
} from '../../../src/tools/local_ripgrep/ripgrepResultBuilder.js';
import type { RipgrepQuery } from '../../../src/tools/local_ripgrep/scheme.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';

const SYMBOL = 'getUser';

// 20 files × 2 matches each — the worst case for any "per-match LSP next" bloat.
// Every match would infer the same symbol, so a per-match implementation would
// emit ~40 lspDefinition/lspReferences blocks. The correct (current) design emits
// exactly one, anchored to the top-ranked file.
const manyFiles = (): LocalSearchCodeFile[] =>
  Array.from(
    { length: 20 },
    (_, i) =>
      ({
        path: `src/file${i}.ts`,
        matchCount: 2,
        matches: [
          { line: 10, column: 0, value: `function ${SYMBOL}() {}` },
          { line: 20, column: 0, value: `  ${SYMBOL}();` },
        ],
      }) as unknown as LocalSearchCodeFile
  );

const query = {
  keywords: SYMBOL,
  sort: 'relevance',
} as unknown as RipgrepQuery;

type ResultShape = {
  files: Array<{ path: string }>;
  next?: {
    lspDefinition?: { query?: Record<string, unknown> };
    lspReferences?: { query?: Record<string, unknown> };
  };
};

describe('localSearchCode LSP next continuation — envelope-level invariant', () => {
  it('emits the LSP next once per response, anchored to the top file (never per-match)', async () => {
    const result = (await buildSearchResult(
      manyFiles(),
      query,
      'rg' as LocalSearchEngine,
      []
    )) as unknown as ResultShape;

    // The continuation exists (guards against a vacuous test) and is anchored to
    // the FIRST (top-ranked) file — not file #19, not duplicated per match.
    expect(result.next?.lspDefinition?.query).toMatchObject({
      symbolName: SYMBOL,
      uri: result.files[0]!.path,
    });
    expect(result.next?.lspReferences?.query).toMatchObject({
      uri: result.files[0]!.path,
    });

    // ...and appears EXACTLY ONCE across the whole envelope, regardless of the
    // 20 files / 40 matches. This is the regression guard: moving the LSP
    // continuation per-match would make these counts 20–40 and fail here.
    const serialized = JSON.stringify(result);
    expect(serialized.match(/"lspDefinition"/g)).toHaveLength(1);
    expect(serialized.match(/"lspReferences"/g)).toHaveLength(1);
  });
});
