import { describe, expect, it } from 'vitest';

import { paginateEntries } from '../../../src/tools/local_view_structure/structureResponse.js';
import type { DirectoryEntry } from '../../../src/tools/local_view_structure/structureFilters.js';

const makeEntries = (count: number): DirectoryEntry[] =>
  Array.from(
    { length: count },
    (_, i) =>
      ({
        name: `file${i}.ts`,
        type: 'file',
      }) as unknown as DirectoryEntry
  );

describe('paginateEntries out-of-range page', () => {
  it('silently clamping to the last page must be flagged, not left unexplained', () => {
    const entries = makeEntries(4);
    const { pagination, paginatedEntries } = paginateEntries(entries, {
      itemsPerPage: 2,
      page: 50,
    });

    // Clamping to the last real page (2) is a reasonable behavior to KEEP —
    // but doing so silently, with no signal that page:50 never existed, lets
    // a caller believe it received page 50's content.
    expect(pagination.totalPages).toBe(2);
    expect(pagination.currentPage).toBe(2);
    expect(paginatedEntries).toHaveLength(2);
    expect((pagination as { outOfRange?: boolean }).outOfRange).toBe(true);
  });

  it('a valid page has no out-of-range signal', () => {
    const entries = makeEntries(4);
    const { pagination } = paginateEntries(entries, {
      itemsPerPage: 2,
      page: 2,
    });

    expect(pagination.currentPage).toBe(2);
    expect((pagination as { outOfRange?: boolean }).outOfRange).toBeFalsy();
  });
});
