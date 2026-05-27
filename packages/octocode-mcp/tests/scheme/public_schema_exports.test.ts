/**
 * Tests to verify all Zod schemas are properly exported from public.ts
 * and contain valid schema definitions with all expected fields.
 */
import { describe, it, expect } from 'vitest';
import { z } from 'zod/v4';

// Import all schemas from public API
import {
  // GitHub Single Query Schemas
  GitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema,
  GitHubPullRequestSearchQuerySchema,
  FileContentQuerySchema,
  // GitHub Bulk Query Schemas
  GitHubCodeSearchBulkQuerySchema,
  GitHubViewRepoStructureBulkQuerySchema,
  GitHubReposSearchQuerySchema,
  GitHubPullRequestSearchBulkQuerySchema,
  FileContentBulkQuerySchema,
  // Local Single Query Schemas
  RipgrepQuerySchema,
  FetchContentQuerySchema,
  FindFilesQuerySchema,
  ViewStructureQuerySchema,
  // Local Bulk Query Schemas
  BulkRipgrepQuerySchema,
  BulkFetchContentSchema,
  BulkFindFilesSchema,
  BulkViewStructureSchema,
  // LSP Single Query Schemas
  LSPGotoDefinitionQuerySchema,
  LSPFindReferencesQuerySchema,
  LSPCallHierarchyQuerySchema,
  // LSP Bulk Query Schemas
  BulkLSPGotoDefinitionSchema,
  BulkLSPFindReferencesSchema,
  BulkLSPCallHierarchySchema,
  // Package Search Schemas
  PackageSearchQuerySchema,
  NpmPackageQuerySchema,
  PackageSearchBulkQuerySchema,
  // Base Schemas & Utilities
  BaseQuerySchema,
  BaseQuerySchemaLocal,
  createBulkQuerySchema,
} from '@octocodeai/octocode-core';

describe('Public Schema Exports', () => {
  describe('Zod Schema Type Verification', () => {
    it('should export valid Zod schemas for all GitHub tools', () => {
      // Single query schemas
      expect(GitHubCodeSearchQuerySchema).toBeDefined();
      expect(GitHubCodeSearchQuerySchema instanceof z.ZodType).toBe(true);

      expect(GitHubViewRepoStructureQuerySchema).toBeDefined();
      expect(GitHubViewRepoStructureQuerySchema instanceof z.ZodType).toBe(
        true
      );

      expect(GitHubReposSearchSingleQuerySchema).toBeDefined();
      expect(GitHubReposSearchSingleQuerySchema instanceof z.ZodType).toBe(
        true
      );

      expect(GitHubPullRequestSearchQuerySchema).toBeDefined();
      expect(GitHubPullRequestSearchQuerySchema instanceof z.ZodType).toBe(
        true
      );

      expect(FileContentQuerySchema).toBeDefined();
      expect(FileContentQuerySchema instanceof z.ZodType).toBe(true);

      // Bulk query schemas
      expect(GitHubCodeSearchBulkQuerySchema).toBeDefined();
      expect(GitHubCodeSearchBulkQuerySchema instanceof z.ZodType).toBe(true);

      expect(GitHubViewRepoStructureBulkQuerySchema).toBeDefined();
      expect(GitHubViewRepoStructureBulkQuerySchema instanceof z.ZodType).toBe(
        true
      );

      expect(GitHubReposSearchQuerySchema).toBeDefined();
      expect(GitHubReposSearchQuerySchema instanceof z.ZodType).toBe(true);

      expect(GitHubPullRequestSearchBulkQuerySchema).toBeDefined();
      expect(GitHubPullRequestSearchBulkQuerySchema instanceof z.ZodType).toBe(
        true
      );

      expect(FileContentBulkQuerySchema).toBeDefined();
      expect(FileContentBulkQuerySchema instanceof z.ZodType).toBe(true);
    });

    it('should export valid Zod schemas for all Local tools', () => {
      // Single query schemas
      expect(RipgrepQuerySchema).toBeDefined();
      expect(RipgrepQuerySchema instanceof z.ZodType).toBe(true);

      expect(FetchContentQuerySchema).toBeDefined();
      expect(FetchContentQuerySchema instanceof z.ZodType).toBe(true);

      expect(FindFilesQuerySchema).toBeDefined();
      expect(FindFilesQuerySchema instanceof z.ZodType).toBe(true);

      expect(ViewStructureQuerySchema).toBeDefined();
      expect(ViewStructureQuerySchema instanceof z.ZodType).toBe(true);

      // Bulk query schemas
      expect(BulkRipgrepQuerySchema).toBeDefined();
      expect(BulkRipgrepQuerySchema instanceof z.ZodType).toBe(true);

      expect(BulkFetchContentSchema).toBeDefined();
      expect(BulkFetchContentSchema instanceof z.ZodType).toBe(true);

      expect(BulkFindFilesSchema).toBeDefined();
      expect(BulkFindFilesSchema instanceof z.ZodType).toBe(true);

      expect(BulkViewStructureSchema).toBeDefined();
      expect(BulkViewStructureSchema instanceof z.ZodType).toBe(true);
    });

    it('should export valid Zod schemas for all LSP tools', () => {
      // Single query schemas
      expect(LSPGotoDefinitionQuerySchema).toBeDefined();
      expect(LSPGotoDefinitionQuerySchema instanceof z.ZodType).toBe(true);

      expect(LSPFindReferencesQuerySchema).toBeDefined();
      expect(LSPFindReferencesQuerySchema instanceof z.ZodType).toBe(true);

      expect(LSPCallHierarchyQuerySchema).toBeDefined();
      expect(LSPCallHierarchyQuerySchema instanceof z.ZodType).toBe(true);

      // Bulk query schemas
      expect(BulkLSPGotoDefinitionSchema).toBeDefined();
      expect(BulkLSPGotoDefinitionSchema instanceof z.ZodType).toBe(true);

      expect(BulkLSPFindReferencesSchema).toBeDefined();
      expect(BulkLSPFindReferencesSchema instanceof z.ZodType).toBe(true);

      expect(BulkLSPCallHierarchySchema).toBeDefined();
      expect(BulkLSPCallHierarchySchema instanceof z.ZodType).toBe(true);
    });

    it('should export valid Zod schemas for Package Search', () => {
      expect(PackageSearchQuerySchema).toBeDefined();
      expect(PackageSearchQuerySchema instanceof z.ZodType).toBe(true);

      expect(NpmPackageQuerySchema).toBeDefined();
      expect(NpmPackageQuerySchema instanceof z.ZodType).toBe(true);

      expect(PackageSearchBulkQuerySchema).toBeDefined();
      expect(PackageSearchBulkQuerySchema instanceof z.ZodType).toBe(true);
    });

    it('should export valid Base schemas and utilities', () => {
      expect(BaseQuerySchema).toBeDefined();
      expect(BaseQuerySchema instanceof z.ZodType).toBe(true);

      expect(BaseQuerySchemaLocal).toBeDefined();
      expect(BaseQuerySchemaLocal instanceof z.ZodType).toBe(true);

      expect(createBulkQuerySchema).toBeDefined();
      expect(typeof createBulkQuerySchema).toBe('function');
    });
  });

  describe('Schema Content Verification - GitHub Tools', () => {
    it('GitHubCodeSearchQuerySchema should have required fields', () => {
      const validQuery = {
        id: 'github_code_query',
        mainResearchGoal: 'Find authentication code',
        researchGoal: 'Locate login implementations',
        reasoning: 'Need to understand auth flow',
        keywordsToSearch: ['authenticate'],
        charOffset: 100,
        charLength: 1200,
      };

      const result = GitHubCodeSearchQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('GitHubCodeSearchBulkQuerySchema should accept queries array', () => {
      const validBulk = {
        responseCharOffset: 100,
        responseCharLength: 2000,
        queries: [
          {
            id: 'github_code_bulk_query',
            mainResearchGoal: 'Find code patterns',
            researchGoal: 'Locate implementations',
            reasoning: 'Research',
            keywordsToSearch: ['function'],
          },
        ],
      };

      const result = GitHubCodeSearchBulkQuerySchema.safeParse(validBulk);
      expect(result.success).toBe(true);
    });

    it('LSPFindReferencesQuerySchema should accept pagination fields', () => {
      const validQuery = {
        id: 'refs_query',
        researchGoal: 'Find symbol usages',
        reasoning: 'Need follow-up references',
        uri: '/tmp/file.ts',
        symbolName: 'foo',
        lineHint: 1,
        referencesPerPage: 20,
        page: 1,
      };

      const result = LSPFindReferencesQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('FileContentQuerySchema should have required GitHub fields', () => {
      const validQuery = {
        id: 'github_file_query',
        mainResearchGoal: 'Read file content',
        researchGoal: 'Get source code',
        reasoning: 'Need implementation details',
        owner: 'octocat',
        repo: 'hello-world',
        path: 'README.md',
      };

      const result = FileContentQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });
  });

  describe('Schema Content Verification - Local Tools', () => {
    it('RipgrepQuerySchema should have required fields', () => {
      const validQuery = {
        id: 'ripgrep_query',
        researchGoal: 'Find exports',
        reasoning: 'Pattern matches export statements',
        pattern: 'export function',
        path: '/src',
      };

      const result = RipgrepQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('BulkRipgrepQuerySchema should accept queries array', () => {
      const validBulk = {
        queries: [
          {
            id: 'ripgrep_bulk_query_1',
            researchGoal: 'Find imports',
            reasoning: 'Pattern matches import statements',
            pattern: 'import',
            path: '/src',
          },
          {
            id: 'ripgrep_bulk_query_2',
            researchGoal: 'Find exports',
            reasoning: 'Pattern matches export statements',
            pattern: 'export',
            path: '/lib',
          },
        ],
      };

      const result = BulkRipgrepQuerySchema.safeParse(validBulk);
      expect(result.success).toBe(true);
    });

    it('FetchContentQuerySchema should have required path field', () => {
      const validQuery = {
        id: 'fetch_content_query',
        researchGoal: 'Read file',
        reasoning: 'Need file content',
        path: '/path/to/file.ts',
      };

      const result = FetchContentQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('FindFilesQuerySchema should have required path field', () => {
      const validQuery = {
        id: 'find_files_query',
        researchGoal: 'Find files',
        reasoning: 'Browse project structure',
        path: '/project',
      };

      const result = FindFilesQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('ViewStructureQuerySchema should have required path field', () => {
      const validQuery = {
        id: 'view_structure_query',
        researchGoal: 'Browse structure',
        reasoning: 'View project layout',
        path: '/project',
      };

      const result = ViewStructureQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });
  });

  describe('Schema Content Verification - LSP Tools', () => {
    it('LSPGotoDefinitionQuerySchema should have required LSP fields', () => {
      const validQuery = {
        id: 'lsp_definition_query',
        researchGoal: 'Find definition',
        reasoning: 'Navigate to symbol',
        uri: 'file:///path/to/file.ts',
        symbolName: 'myFunction',
        lineHint: 10,
      };

      const result = LSPGotoDefinitionQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('LSPFindReferencesQuerySchema should have required LSP fields', () => {
      const validQuery = {
        id: 'lsp_references_query',
        researchGoal: 'Find usages',
        reasoning: 'Trace symbol references',
        uri: 'file:///path/to/file.ts',
        symbolName: 'MyType',
        lineHint: 5,
      };

      const result = LSPFindReferencesQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('LSPCallHierarchyQuerySchema should have required fields including direction', () => {
      const validQuery = {
        id: 'lsp_call_hierarchy_query',
        researchGoal: 'Trace calls',
        reasoning: 'Analyze call flow',
        uri: 'file:///path/to/file.ts',
        symbolName: 'handleRequest',
        lineHint: 20,
        direction: 'incoming',
      };

      const result = LSPCallHierarchyQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('BulkLSPCallHierarchySchema should accept queries array', () => {
      const validBulk = {
        queries: [
          {
            id: 'lsp_call_hierarchy_bulk_1',
            researchGoal: 'Trace func1',
            reasoning: 'Find callers',
            uri: 'file:///path/to/file.ts',
            symbolName: 'func1',
            lineHint: 10,
            direction: 'incoming',
          },
          {
            id: 'lsp_call_hierarchy_bulk_2',
            researchGoal: 'Trace func2',
            reasoning: 'Find callees',
            uri: 'file:///path/to/file.ts',
            symbolName: 'func2',
            lineHint: 20,
            direction: 'outgoing',
          },
        ],
      };

      const result = BulkLSPCallHierarchySchema.safeParse(validBulk);
      expect(result.success).toBe(true);
    });
  });

  describe('Schema Content Verification - Package Search', () => {
    it('NpmPackageQuerySchema should accept npm ecosystem', () => {
      const validQuery = {
        id: 'npm_package_query',
        mainResearchGoal: 'Find npm package',
        researchGoal: 'Get package info',
        reasoning: 'Need to check package',
        name: 'lodash',
        ecosystem: 'npm',
      };

      const result = NpmPackageQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('PackageSearchBulkQuerySchema should accept queries array', () => {
      const validBulk = {
        queries: [
          {
            id: 'package_bulk_query',
            mainResearchGoal: 'Find packages',
            researchGoal: 'Get package info',
            reasoning: 'Research',
            name: 'express',
            ecosystem: 'npm',
          },
        ],
      };

      const result = PackageSearchBulkQuerySchema.safeParse(validBulk);
      expect(result.success).toBe(true);
    });
  });

  describe('Base Schema Verification', () => {
    it('BaseQuerySchema should have research fields', () => {
      const validQuery = {
        id: 'base_query',
        mainResearchGoal: 'Main goal',
        researchGoal: 'Specific goal',
        reasoning: 'Why this approach',
      };

      const result = BaseQuerySchema.safeParse(validQuery);
      expect(result.success).toBe(true);
    });

    it('BaseQuerySchema should reject missing required fields', () => {
      const invalidQuery = {
        id: 'missing_fields',
        mainResearchGoal: 'Only main goal',
      };

      const result = BaseQuerySchema.safeParse(invalidQuery);
      expect(result.success).toBe(false);
    });

    it('BaseQuerySchema should reject missing id', () => {
      const invalidQuery = {
        mainResearchGoal: 'Main goal',
        researchGoal: 'Specific goal',
        reasoning: 'Why this approach',
      };

      const result = BaseQuerySchema.safeParse(invalidQuery);
      expect(result.success).toBe(false);
    });

    it('BaseQuerySchemaLocal should require researchGoal and reasoning', () => {
      const result = BaseQuerySchemaLocal.safeParse({});
      expect(result.success).toBe(false);
    });

    it('BaseQuerySchemaLocal should accept valid research fields', () => {
      const result = BaseQuerySchemaLocal.safeParse({
        id: 'local_query',
        researchGoal: 'Find X',
        reasoning: 'Search pattern helps',
      });
      expect(result.success).toBe(true);
    });
  });

  describe('createBulkQuerySchema Utility', () => {
    it('should create a valid bulk schema from single query schema', () => {
      const singleSchema = z.object({
        query: z.string(),
      });

      const bulkSchema = createBulkQuerySchema('testTool', singleSchema);

      expect(bulkSchema).toBeDefined();
      expect(bulkSchema instanceof z.ZodType).toBe(true);
    });

    it('should validate queries array with min 1 max 3 by default', () => {
      const singleSchema = z.object({
        query: z.string(),
      });

      const bulkSchema = createBulkQuerySchema('testTool', singleSchema);

      // Valid: 1 query
      const valid1 = bulkSchema.safeParse({ queries: [{ query: 'test' }] });
      expect(valid1.success).toBe(true);

      // Valid: 3 queries
      const valid3 = bulkSchema.safeParse({
        queries: [{ query: 'a' }, { query: 'b' }, { query: 'c' }],
      });
      expect(valid3.success).toBe(true);

      // Invalid: 0 queries
      const invalid0 = bulkSchema.safeParse({ queries: [] });
      expect(invalid0.success).toBe(false);

      // Invalid: 4 queries (exceeds max)
      const invalid4 = bulkSchema.safeParse({
        queries: [
          { query: 'a' },
          { query: 'b' },
          { query: 'c' },
          { query: 'd' },
        ],
      });
      expect(invalid4.success).toBe(false);
    });

    it('should respect custom maxQueries option', () => {
      const singleSchema = z.object({
        value: z.number(),
      });

      const bulkSchema = createBulkQuerySchema('testTool', singleSchema, {
        maxQueries: 5,
      });

      // Valid: 5 queries
      const valid5 = bulkSchema.safeParse({
        queries: [
          { value: 1 },
          { value: 2 },
          { value: 3 },
          { value: 4 },
          { value: 5 },
        ],
      });
      expect(valid5.success).toBe(true);

      // Invalid: 6 queries
      const invalid6 = bulkSchema.safeParse({
        queries: [
          { value: 1 },
          { value: 2 },
          { value: 3 },
          { value: 4 },
          { value: 5 },
          { value: 6 },
        ],
      });
      expect(invalid6.success).toBe(false);
    });

    it('should reject duplicate query ids within the same bulk call', () => {
      const singleSchema = z.object({
        id: z.string().optional(),
        query: z.string(),
      });

      const bulkSchema = createBulkQuerySchema('testTool', singleSchema);

      const duplicateIds = bulkSchema.safeParse({
        queries: [
          { id: 'shared_id', query: 'a' },
          { id: 'shared_id', query: 'b' },
        ],
      });

      expect(duplicateIds.success).toBe(false);
      if (!duplicateIds.success) {
        expect(
          duplicateIds.error.issues.some(issue =>
            String(issue.message).includes('Duplicate query id "shared_id"')
          )
        ).toBe(true);
      }
    });
  });

  describe('Schema Validation Behavior', () => {
    it('should reject invalid data for single query schemas', () => {
      // Missing required pattern field
      const invalidRipgrep = RipgrepQuerySchema.safeParse({
        researchGoal: 'Test',
        reasoning: 'Test',
        path: '/src',
      });
      expect(invalidRipgrep.success).toBe(false);

      // Missing required uri field
      const invalidLSP = LSPGotoDefinitionQuerySchema.safeParse({
        symbolName: 'test',
        lineHint: 1,
      });
      expect(invalidLSP.success).toBe(false);
    });

    it('should reject invalid data for bulk query schemas', () => {
      // Missing queries field
      const invalidBulk = BulkRipgrepQuerySchema.safeParse({});
      expect(invalidBulk.success).toBe(false);

      // Invalid query in array
      const invalidQueryInArray = BulkRipgrepQuerySchema.safeParse({
        queries: [{ invalid: 'data' }],
      });
      expect(invalidQueryInArray.success).toBe(false);
    });
  });

  describe('Schema Default Values', () => {
    it('RipgrepQuerySchema should apply default values', () => {
      const query = {
        id: 'ripgrep_defaults_query',
        researchGoal: 'Test',
        reasoning: 'Schema validation',
        pattern: 'test',
        path: '/src',
      };

      const result = RipgrepQuerySchema.parse(query);

      // Check default values are applied
      expect(result.smartCase).toBe(true);
      expect(result.matchContentLength).toBe(200);
      expect(result.filesPerPage).toBe(10);
      expect(result.filePageNumber).toBe(1);
      expect(result.matchesPerPage).toBe(10);
      expect(result.binaryFiles).toBe('without-match');
      expect(result.includeStats).toBe(true);
      expect(result.sort).toBe('path');
      expect(result.showFileLastModified).toBe(false);
    });

    it('ViewStructureQuerySchema should apply default values', () => {
      const query = {
        id: 'view_structure_defaults_query',
        researchGoal: 'Browse',
        reasoning: 'View structure',
        path: '/project',
      };

      const result = ViewStructureQuerySchema.parse(query);

      expect(result.details).toBe(false);
      expect(result.hidden).toBe(false);
      expect(result.humanReadable).toBe(true);
      expect(result.sortBy).toBe('time');
      expect(result.entriesPerPage).toBe(20);
      expect(result.entryPageNumber).toBe(1);
      expect(result.showFileLastModified).toBe(true);
    });

    it('LSPCallHierarchyQuerySchema should apply default values', () => {
      const query = {
        id: 'lsp_call_hierarchy_defaults_query',
        researchGoal: 'Trace calls',
        reasoning: 'Find callers',
        uri: 'file:///test.ts',
        symbolName: 'myFunc',
        lineHint: 10,
        direction: 'incoming',
      };

      const result = LSPCallHierarchyQuerySchema.parse(query);

      expect(result.depth).toBe(1);
      expect(result.contextLines).toBe(2);
      expect(result.callsPerPage).toBe(15);
      expect(result.page).toBe(1);
      expect(result.orderHint).toBe(0);
    });
  });

  describe('Complete Export Inventory', () => {
    it('should export all 13 single query schemas', () => {
      const singleSchemas = [
        GitHubCodeSearchQuerySchema,
        GitHubViewRepoStructureQuerySchema,
        GitHubReposSearchSingleQuerySchema,
        GitHubPullRequestSearchQuerySchema,
        FileContentQuerySchema,
        RipgrepQuerySchema,
        FetchContentQuerySchema,
        FindFilesQuerySchema,
        ViewStructureQuerySchema,
        LSPGotoDefinitionQuerySchema,
        LSPFindReferencesQuerySchema,
        LSPCallHierarchyQuerySchema,
        NpmPackageQuerySchema,
      ];

      singleSchemas.forEach(schema => {
        expect(schema).toBeDefined();
        expect(schema instanceof z.ZodType).toBe(true);
      });

      expect(singleSchemas.length).toBe(13);
    });

    it('should export all 14 bulk query schemas', () => {
      const bulkSchemas = [
        GitHubCodeSearchBulkQuerySchema,
        GitHubViewRepoStructureBulkQuerySchema,
        GitHubReposSearchQuerySchema,
        GitHubPullRequestSearchBulkQuerySchema,
        FileContentBulkQuerySchema,
        BulkRipgrepQuerySchema,
        BulkFetchContentSchema,
        BulkFindFilesSchema,
        BulkViewStructureSchema,
        BulkLSPGotoDefinitionSchema,
        BulkLSPFindReferencesSchema,
        BulkLSPCallHierarchySchema,
        PackageSearchQuerySchema, // discriminated union of single queries
        PackageSearchBulkQuerySchema,
      ];

      bulkSchemas.forEach(schema => {
        expect(schema).toBeDefined();
        expect(schema instanceof z.ZodType).toBe(true);
      });

      expect(bulkSchemas.length).toBe(14);
    });

    it('should export 2 base schemas and 1 utility function', () => {
      expect(BaseQuerySchema).toBeDefined();
      expect(BaseQuerySchema instanceof z.ZodType).toBe(true);

      expect(BaseQuerySchemaLocal).toBeDefined();
      expect(BaseQuerySchemaLocal instanceof z.ZodType).toBe(true);

      expect(createBulkQuerySchema).toBeDefined();
      expect(typeof createBulkQuerySchema).toBe('function');
    });
  });
});
