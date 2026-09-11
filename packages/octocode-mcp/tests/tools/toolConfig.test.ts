import { readFileSync } from 'node:fs';
import { describe, it, expect } from 'vitest';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';
import { TOOL_NAMES } from '../../../octocode-tools-core/src/tools/toolMetadata/names.js';
import { PUBLIC_TOOL_DESCRIPTIONS } from '@octocodeai/octocode-core/schema';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '@octocodeai/octocode-core/schema';
import {
  GITHUB_SEARCH_TOOL_NAME,
  LOCAL_SEARCH_TOOL_NAME,
} from '@octocodeai/octocode-core/schema';
import { LSP_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';

const removedLspToolNames = [
  `lsp${'Goto'}Definition`,
  `lsp${'Find'}References`,
  `lsp${'Call'}Hierarchy`,
];

describe('Tool Configuration', () => {
  function tool(name: string) {
    const found = ALL_TOOLS.find(candidate => candidate.name === name);
    expect(found, `missing tool ${name}`).toBeDefined();
    return found!;
  }

  describe('ALL_TOOLS', () => {
    it('contains the 11 default-enabled tools without legacy aliases', () => {
      expect(ALL_TOOLS).toHaveLength(11);

      const toolNames = ALL_TOOLS.map(t => t.name);
      expect(toolNames).toEqual([
        GITHUB_SEARCH_TOOL_NAME,
        TOOL_NAMES.GITHUB_FETCH_CONTENT,
        TOOL_NAMES.GITHUB_SEARCH_HISTORY,
        TOOL_NAMES.GITHUB_GET_HISTORY_ITEM,
        TOOL_NAMES.PACKAGE_SEARCH,
        TOOL_NAMES.GITHUB_CLONE_REPO,
        LOCAL_SEARCH_TOOL_NAME,
        'astSearch',
        'astRewrite',
        TOOL_NAMES.LOCAL_FETCH_CONTENT,
        'lspSearch',
      ]);

      expect(toolNames).not.toContain('ghListReleases');
      expect(toolNames).not.toContain('ghSearchDiscussions');
      for (const legacyName of [
        'github.code',
        'github.repositories',
        'github.tree',
        'local.text',
        'localAnalyzeGraph',
        'lspGetSemantics',
      ]) {
        expect(toolNames).not.toContain(legacyName);
      }
      for (const removedName of removedLspToolNames) {
        expect(toolNames).not.toContain(removedName);
      }
    });

    it('publishes the same 11-tool runtime and discovery catalog', () => {
      expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS).toHaveLength(11);
      expect(ALL_TOOLS.filter(tool => tool.isDefault)).toHaveLength(11);
      expect(DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => tool.name)).toEqual(
        ALL_TOOLS.map(tool => tool.name)
      );
    });

    it('uses shared titles without an MCP-local title registry', () => {
      const source = readFileSync(
        new URL('../../src/tools/toolConfig.ts', import.meta.url),
        'utf8'
      );
      expect(source).not.toContain('MCP_TOOL_TITLES');
      expect(source).not.toContain('requireTool');
      expect(ALL_TOOLS.every(tool => tool.title.trim().length > 0)).toBe(true);
    });

    it('should have valid tool types', () => {
      const validTypes = ['search', 'content', 'history', 'debug'];
      ALL_TOOLS.forEach(tool => {
        expect(validTypes).toContain(tool.type);
      });
    });

    it('should have isLocal correctly set for GitHub tools', () => {
      const remoteCapableTools = ALL_TOOLS.filter(t => !t.isLocal);
      expect(remoteCapableTools).toHaveLength(5);
      remoteCapableTools.forEach(tool => {
        expect(tool.isLocal).toBe(false);
      });
    });

    it('should have isLocal correctly set for Local tools', () => {
      const localTools = ALL_TOOLS.filter(t => t.isLocal);
      expect(localTools).toHaveLength(6);
      localTools.forEach(tool => {
        expect(tool.isLocal).toBe(true);
      });
    });
  });

  describe('GitHub tool configs', () => {
    it('GITHUB_FETCH_CONTENT should have correct config', () => {
      const GITHUB_FETCH_CONTENT = tool(TOOL_NAMES.GITHUB_FETCH_CONTENT);
      expect(GITHUB_FETCH_CONTENT.name).toBe(TOOL_NAMES.GITHUB_FETCH_CONTENT);
      expect(GITHUB_FETCH_CONTENT.description).toBe(
        PUBLIC_TOOL_DESCRIPTIONS[TOOL_NAMES.GITHUB_FETCH_CONTENT]
      );
      expect(GITHUB_FETCH_CONTENT.type).toBe('content');
      expect(GITHUB_FETCH_CONTENT.isLocal).toBe(false);
      expect(GITHUB_FETCH_CONTENT.fn).toBeTypeOf('function');
    });

    it('GITHUB_SEARCH should have the unified public config', () => {
      const githubSearch = ALL_TOOLS.find(
        tool => tool.name === GITHUB_SEARCH_TOOL_NAME
      );
      expect(githubSearch).toMatchObject({
        name: GITHUB_SEARCH_TOOL_NAME,
        type: 'search',
        isLocal: false,
        isDefault: true,
      });
      expect(githubSearch?.fn).toBeTypeOf('function');
    });

    it('PACKAGE_SEARCH should have correct config', () => {
      const PACKAGE_SEARCH = tool(TOOL_NAMES.PACKAGE_SEARCH);
      expect(PACKAGE_SEARCH.name).toBe(TOOL_NAMES.PACKAGE_SEARCH);
      expect(PACKAGE_SEARCH.description).toBe(
        PUBLIC_TOOL_DESCRIPTIONS[TOOL_NAMES.PACKAGE_SEARCH]
      );
      expect(PACKAGE_SEARCH.type).toBe('search');
      expect(PACKAGE_SEARCH.isLocal).toBe(false);
      expect(PACKAGE_SEARCH.fn).toBeTypeOf('function');
    });
  });

  describe('Local tool configs', () => {
    it('LOCAL_SEARCH should expose lexical text search', () => {
      const LOCAL_SEARCH = tool(LOCAL_SEARCH_TOOL_NAME);
      expect(LOCAL_SEARCH.name).toBe(LOCAL_SEARCH_TOOL_NAME);
      expect(LOCAL_SEARCH.type).toBe('search');
      expect(LOCAL_SEARCH.isLocal).toBe(true);
      expect(LOCAL_SEARCH.fn).toBeTypeOf('function');
      expect(
        LOCAL_SEARCH.direct.schema.safeParse({
          path: '.',
          searchText: 'needle',
          regex: 'literal',
        }).success
      ).toBe(true);
    });

    it('LOCAL_FETCH_CONTENT should have correct config', () => {
      const LOCAL_FETCH_CONTENT = tool(TOOL_NAMES.LOCAL_FETCH_CONTENT);
      expect(LOCAL_FETCH_CONTENT.name).toBe(TOOL_NAMES.LOCAL_FETCH_CONTENT);
      expect(LOCAL_FETCH_CONTENT.type).toBe('content');
      expect(LOCAL_FETCH_CONTENT.isLocal).toBe(true);
      expect(LOCAL_FETCH_CONTENT.fn).toBeTypeOf('function');
    });

    it('LSP semantic tool should have correct config', () => {
      const LSP_SEARCH = tool(LSP_SEARCH_TOOL_NAME);
      expect(LSP_SEARCH.name).toBe(LSP_SEARCH_TOOL_NAME);
      expect(LSP_SEARCH.type).toBe('content');
      expect(LSP_SEARCH.isLocal).toBe(true);
    });
  });

  describe('Clone tool config', () => {
    it('GITHUB_CLONE_REPO should have isClone: true', () => {
      const GITHUB_CLONE_REPO = tool(TOOL_NAMES.GITHUB_CLONE_REPO);
      expect(GITHUB_CLONE_REPO.isClone).toBe(true);
    });

    it('GITHUB_CLONE_REPO should have isLocal: true', () => {
      const GITHUB_CLONE_REPO = tool(TOOL_NAMES.GITHUB_CLONE_REPO);
      expect(GITHUB_CLONE_REPO.isLocal).toBe(true);
    });

    it('only GITHUB_CLONE_REPO should have isClone: true', () => {
      const cloneTools = ALL_TOOLS.filter(t => t.isClone);
      expect(cloneTools).toHaveLength(1);
      expect(cloneTools[0]!.name).toBe(TOOL_NAMES.GITHUB_CLONE_REPO);
    });

    it('non-clone tools should not have isClone set', () => {
      const nonCloneTools = ALL_TOOLS.filter(t => !t.isClone);
      expect(nonCloneTools).toHaveLength(10);
      nonCloneTools.forEach(tool => {
        expect(tool.isClone).toBeFalsy();
      });
    });
  });
});
