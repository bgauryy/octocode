import { describe, expect, it } from 'vitest';
import { NpmPackageQuerySchema } from '../../src/toolContract/input/resources/tools/npmSearch.js';

describe('npmSearch exact and discovery selectors', () => {
  it('normalizes names and discovery terms without guessing the mode from their shape', () => {
    expect(
      NpmPackageQuerySchema.parse({ packageName: ' @acme/widget ' })
    ).toMatchObject({ packageName: '@acme/widget' });
    expect(
      NpmPackageQuerySchema.parse({ keywords: [' zod '], page: 2 })
    ).toMatchObject({ keywords: ['zod'], page: 2 });
  });

  it.each([
    {},
    { packageName: ' ' },
    { keywords: [] },
    { keywords: [' '] },
    { packageName: 'zod', keywords: [] },
    { packageName: 'zod', keywords: ['schema'] },
    { packageName: 'zod', page: 1 },
    { packageName: 'zod', page: 2 },
  ])('rejects empty, ambiguous, or inapplicable selectors: %j', query => {
    expect(NpmPackageQuerySchema.safeParse(query).success).toBe(false);
  });
});
