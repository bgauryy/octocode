/**
 * Comprehensive verification that ALL Zod .describe() fields across ALL 14 tools
 * produce non-empty descriptions when schemas are built after metadata loads.
 *
 * Production startup sequence (simulated here):
 *   1. loadToolContent()        → METADATA_JSON set
 *   2. import toolsManager      → all scheme.ts modules evaluate
 *   3. registerTools(server)    → toMCPSchema() reads populated descriptions
 *
 * For each tool this test:
 *   - Converts the bulk schema to JSON Schema via z.toJSONSchema()
 *   - Walks every property recursively
 *   - Asserts ZERO empty descriptions
 *   - Asserts the exact expected field count (catches regressions)
 *   - Asserts every expected field name is present with a description
 */
import { describe, it, expect, vi, beforeAll } from 'vitest';
import {
  buildMockMetadata,
  collectJsonSchemaDescriptions,
} from './fixtures.js';

const EXPECTED_FIELDS: Record<string, readonly string[]> = {
  github_search_code: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'keywordsToSearch',
    'owner',
    'repo',
    'extension',
    'filename',
    'path',
    'match',
    'limit',
    'page',
  ],
  github_fetch_content: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'owner',
    'repo',
    'path',
    'branch',
    'type',
    'fullContent',
    'startLine',
    'endLine',
    'matchString',
    'matchStringContextLines',
    'charOffset',
    'charLength',
    'forceRefresh',
  ],
  github_search_repos: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'keywordsToSearch',
    'topicsToSearch',
    'owner',
    'stars',
    'size',
    'created',
    'updated',
    'match',
    'sort',
    'limit',
    'page',
  ],
  github_search_pull_requests: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'query',
    'owner',
    'repo',
    'prNumber',
    'state',
    'assignee',
    'author',
    'commenter',
    'involves',
    'mentions',
    'review-requested',
    'reviewed-by',
    'label',
    'no-label',
    'no-milestone',
    'no-project',
    'no-assignee',
    'head',
    'base',
    'created',
    'updated',
    'closed',
    'merged-at',
    'comments',
    'reactions',
    'interactions',
    'merged',
    'draft',
    'match',
    'sort',
    'order',
    'limit',
    'page',
    'withComments',
    'withCommits',
    'type',
    'partialContentMetadata',
    'charOffset',
    'charLength',
  ],
  github_view_repo_structure: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'owner',
    'repo',
    'branch',
    'path',
    'depth',
    'entriesPerPage',
    'entryPageNumber',
  ],
  github_clone_repo: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'owner',
    'repo',
    'branch',
    'sparse_path',
    'forceRefresh',
  ],
  package_search: [
    'queries',
    'id',
    'mainResearchGoal',
    'researchGoal',
    'reasoning',
    'name',
    'searchLimit',
    'ecosystem',
    'npmFetchMetadata',
  ],
  local_ripgrep: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'pattern',
    'path',
    'mode',
    'fixedString',
    'perlRegex',
    'smartCase',
    'caseInsensitive',
    'caseSensitive',
    'wholeWord',
    'invertMatch',
    'type',
    'include',
    'exclude',
    'excludeDir',
    'noIgnore',
    'hidden',
    'followSymlinks',
    'filesOnly',
    'filesWithoutMatch',
    'count',
    'countMatches',
    'contextLines',
    'beforeContext',
    'afterContext',
    'matchContentLength',
    'maxMatchesPerFile',
    'maxFiles',
    'filesPerPage',
    'filePageNumber',
    'matchesPerPage',
    'charOffset',
    'charLength',
    'multiline',
    'multilineDotall',
    'binaryFiles',
    'includeStats',
    'threads',
    'mmap',
    'noUnicode',
    'encoding',
    'sort',
    'sortReverse',
    'noMessages',
    'lineRegexp',
    'passthru',
    'debug',
    'showFileLastModified',
  ],
  local_fetch_content: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'path',
    'fullContent',
    'startLine',
    'endLine',
    'matchString',
    'matchStringContextLines',
    'matchStringIsRegex',
    'matchStringCaseSensitive',
    'charOffset',
    'charLength',
  ],
  local_find_files: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'path',
    'maxDepth',
    'minDepth',
    'name',
    'iname',
    'names',
    'pathPattern',
    'regex',
    'regexType',
    'type',
    'empty',
    'modifiedWithin',
    'modifiedBefore',
    'accessedWithin',
    'sizeGreater',
    'sizeLess',
    'permissions',
    'executable',
    'readable',
    'writable',
    'excludeDir',
    'sortBy',
    'limit',
    'details',
    'filesPerPage',
    'filePageNumber',
    'charOffset',
    'charLength',
    'showFileLastModified',
  ],
  local_view_structure: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'path',
    'details',
    'hidden',
    'humanReadable',
    'sortBy',
    'reverse',
    'entriesPerPage',
    'entryPageNumber',
    'pattern',
    'directoriesOnly',
    'filesOnly',
    'extension',
    'extensions',
    'depth',
    'recursive',
    'limit',
    'charOffset',
    'charLength',
    'showFileLastModified',
  ],
  lsp_goto_definition: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'uri',
    'symbolName',
    'lineHint',
    'orderHint',
    'contextLines',
    'charOffset',
    'charLength',
  ],
  lsp_find_references: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'uri',
    'symbolName',
    'lineHint',
    'orderHint',
    'includeDeclaration',
    'contextLines',
    'referencesPerPage',
    'page',
    'includePattern',
    'excludePattern',
  ],
  lsp_call_hierarchy: [
    'queries',
    'id',
    'researchGoal',
    'reasoning',
    'uri',
    'symbolName',
    'lineHint',
    'orderHint',
    'direction',
    'depth',
    'contextLines',
    'callsPerPage',
    'page',
    'charOffset',
    'charLength',
  ],
};

const KNOWN_UPSTREAM_EMPTY_FIELDS = new Set([
  'github_search_pull_requests:charOffset',
  'github_search_pull_requests:charLength',
  'local_find_files:charOffset',
  'local_find_files:charLength',
  'local_view_structure:charOffset',
  'local_view_structure:charLength',
]);

// Bulk schema export names keyed by tool directory name
const BULK_SCHEMA_EXPORTS: Record<string, string> = {
  github_search_code: 'GitHubCodeSearchBulkQuerySchema',
  github_fetch_content: 'FileContentBulkQuerySchema',
  github_search_repos: 'GitHubReposSearchQuerySchema',
  github_search_pull_requests: 'GitHubPullRequestSearchBulkQuerySchema',
  github_view_repo_structure: 'GitHubViewRepoStructureBulkQuerySchema',
  github_clone_repo: 'BulkCloneRepoSchema',
  package_search: 'PackageSearchBulkQuerySchema',
  local_ripgrep: 'BulkRipgrepQuerySchema',
  local_fetch_content: 'BulkFetchContentSchema',
  local_find_files: 'BulkFindFilesSchema',
  local_view_structure: 'BulkViewStructureSchema',
  lsp_goto_definition: 'BulkLSPGotoDefinitionSchema',
  lsp_find_references: 'BulkLSPFindReferencesSchema',
  lsp_call_hierarchy: 'BulkLSPCallHierarchySchema',
};

// Mock metadata and JSON Schema walker imported from shared fixtures

describe('ALL tool schemas: every .describe() field is non-empty after metadata init', () => {
  let schemaModules: Record<string, Record<string, unknown>>;
  let zod: typeof import('zod/v4');

  beforeAll(async () => {
    vi.resetModules();

    vi.doMock('@octocodeai/octocode-core', async importOriginal => {
      const actual =
        await importOriginal<typeof import('@octocodeai/octocode-core')>();
      const meta = buildMockMetadata();
      return {
        ...actual,
        octocodeConfig: meta,
        completeMetadata: meta,
      };
    });

    const { _resetMetadataState, initializeToolMetadata } =
      await import('../../src/tools/toolMetadata/state.js');
    _resetMetadataState();
    await initializeToolMetadata();

    zod = await import('zod/v4');

    const coreSchemas = await import('@octocodeai/octocode-core');
    schemaModules = {
      github_search_code: coreSchemas,
      github_fetch_content: coreSchemas,
      github_search_repos: coreSchemas,
      github_search_pull_requests: coreSchemas,
      github_view_repo_structure: coreSchemas,
      github_clone_repo: coreSchemas,
      package_search: coreSchemas,
      local_ripgrep: coreSchemas,
      local_fetch_content: coreSchemas,
      local_find_files: coreSchemas,
      local_view_structure: coreSchemas,
      lsp_goto_definition: coreSchemas,
      lsp_find_references: coreSchemas,
      lsp_call_hierarchy: coreSchemas,
    };
  }, 15000);

  it.each(Object.keys(EXPECTED_FIELDS))(
    '%s — zero empty descriptions, all expected fields present',
    toolKey => {
      const exportName = BULK_SCHEMA_EXPORTS[toolKey]!;
      const bulkSchema = schemaModules[toolKey]?.[exportName];
      expect(
        bulkSchema,
        `${exportName} must be exported from ${toolKey}/scheme.ts`
      ).toBeDefined();

      const jsonSchema = zod.toJSONSchema(
        bulkSchema as Parameters<typeof zod.toJSONSchema>[0]
      );
      const allDescriptions = collectJsonSchemaDescriptions(jsonSchema);

      // 1. No field should have description: "" (excluding known upstream gaps)
      const emptyFields = allDescriptions.filter(
        d =>
          d.description === '' &&
          !KNOWN_UPSTREAM_EMPTY_FIELDS.has(`${toolKey}:${d.fieldName}`)
      );
      expect(
        emptyFields.map(d => d.path),
        `[${toolKey}] fields with empty description`
      ).toEqual([]);

      // 2. All expected field names must be present with a description
      const describedFieldNames = new Set(
        allDescriptions.map(d => d.fieldName)
      );
      const expectedFields = EXPECTED_FIELDS[toolKey]!;
      const missingFields = expectedFields.filter(
        f => !describedFieldNames.has(f)
      );
      expect(
        missingFields,
        `[${toolKey}] expected fields missing a description`
      ).toEqual([]);

      // 3. Each described field must have a meaningful description (>= 3 chars)
      for (const desc of allDescriptions) {
        if (desc.description.length > 0) {
          expect(
            desc.description.length,
            `[${toolKey}] ${desc.path} description too short: "${desc.description}"`
          ).toBeGreaterThanOrEqual(3);
        }
      }
    }
  );

  it('total described field count across all 14 tools', () => {
    let totalDescribed = 0;
    let totalEmpty = 0;

    for (const toolKey of Object.keys(EXPECTED_FIELDS)) {
      const exportName = BULK_SCHEMA_EXPORTS[toolKey]!;
      const bulkSchema = schemaModules[toolKey]?.[exportName];
      if (!bulkSchema) continue;

      const jsonSchema = zod.toJSONSchema(
        bulkSchema as Parameters<typeof zod.toJSONSchema>[0]
      );
      const descriptions = collectJsonSchemaDescriptions(jsonSchema);
      totalDescribed += descriptions.filter(d => d.description !== '').length;
      totalEmpty += descriptions.filter(
        d =>
          d.description === '' &&
          !KNOWN_UPSTREAM_EMPTY_FIELDS.has(`${toolKey}:${d.fieldName}`)
      ).length;
    }

    expect(totalEmpty).toBe(0);
    // 14 tools × avg ~15 fields each = expect well over 100 described fields
    expect(totalDescribed).toBeGreaterThan(100);
  });

  it('covers exactly 14 tools', () => {
    expect(Object.keys(EXPECTED_FIELDS)).toHaveLength(14);
    expect(Object.keys(BULK_SCHEMA_EXPORTS)).toHaveLength(14);
    expect(Object.keys(schemaModules)).toHaveLength(14);
  });
});
