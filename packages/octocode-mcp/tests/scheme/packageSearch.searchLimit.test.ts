/**
 * packageSearch page-size is now the cross-tool `itemsPerPage` (default 20).
 * The legacy `limit`/`searchLimit` names were removed — no aliases.
 */
import { describe, it, expect } from 'vitest';
import { PackageSearchBulkQueryLocalSchema } from '../../src/scheme/remoteSchemaOverlay.js';
import { describePackageSearch } from '../../src/tools/package_search/package_search.js';

function parsedQuery(query: Record<string, unknown>): Record<string, unknown> {
  const parsed = PackageSearchBulkQueryLocalSchema.parse({ queries: [query] });
  return parsed.queries[0] as Record<string, unknown>;
}

describe('packageSearch itemsPerPage (no legacy limit/searchLimit)', () => {
  it('honors an explicit itemsPerPage', () => {
    expect(parsedQuery({ name: 'lodash', itemsPerPage: 1 }).itemsPerPage).toBe(
      1
    );
  });

  it('defaults itemsPerPage to 20 when omitted', () => {
    expect(parsedQuery({ name: 'lodash' }).itemsPerPage).toBe(20);
  });

  it('drops a legacy `limit` (no alias) and keeps the itemsPerPage default', () => {
    const q = parsedQuery({ name: 'lodash', limit: 2 });
    expect('limit' in q).toBe(false);
    expect('searchLimit' in q).toBe(false);
    expect(q.itemsPerPage).toBe(20);
  });
});

describe('packageSearch description has no searchLimit drift', () => {
  // The real upstream lines (octocode-core default.json) that reference the
  // removed knob. Kept verbatim so the corrective rename is exercised against
  // the actual prose the agent would otherwise see (the runtime metadata is
  // mocked in tests/setup.ts, so we can't read completeMetadata here).
  const upstreamLines = [
    'Exact-name lookup (name="<pkg>", searchLimit=1, npmFetchMetadata=false) is the cheapest path. Promote searchLimit/metadata only when comparing alternatives or needing repo URL/versions.',
    'concise: 1 record ({name, version, repository, deprecated}); searchLimit capped at 1, npmFetchMetadata forced off.',
    'No row pagination; `searchLimit` is the only result cap.',
    'name="lodash", searchLimit=5',
  ].join('\n');

  it('strips every searchLimit reference from the corrected description', () => {
    expect(describePackageSearch(upstreamLines)).not.toContain('searchLimit');
  });

  it('rewrites searchLimit examples to the real itemsPerPage field', () => {
    const out = describePackageSearch(upstreamLines);
    expect(out).toContain('itemsPerPage=1');
    expect(out).toContain('itemsPerPage=5');
    expect(out).toContain('itemsPerPage capped at 1');
  });

  it('preserves the still-valid npmFetchMetadata field', () => {
    expect(describePackageSearch(upstreamLines)).toContain('npmFetchMetadata');
  });

  it('appends the cross-tool <when> guidance', () => {
    expect(describePackageSearch('base')).toContain('<when>Use packageSearch');
  });
});
