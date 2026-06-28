import { describe, expect, it } from 'vitest';

import {
  DirectToolInputError,
  prepareDirectToolInput,
} from '../../src/tools/directToolCatalog.meta.js';

describe('prepareDirectToolInput', () => {
  it('rejects unknown query fields when strict mode is enabled', () => {
    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow(DirectToolInputError);

    expect(() =>
      prepareDirectToolInput(
        'localSearchCode',
        { path: '.', keywords: 'runCLI', typo: true },
        { rejectUnknownFields: true }
      )
    ).toThrow('Unknown field(s): typo');
  });
});
