import { readFile } from 'node:fs/promises';
import path from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

import * as publicApi from '../../src/index.js';
import { DIRECT_TOOL_DISCOVERY_DEFINITIONS } from '../../src/tools/directToolCatalog/toolCatalogDefinitions.js';

const SOURCE_ROOT = path.resolve(import.meta.dirname, '../../src');
const LEGACY_PUBLIC_NAMES = [
  'local.text',
  'local.files',
  'local.tree',
] as const;
const LEGACY_EXPORTS = [
  'executeRipgrepSearch',
  'executeFindFiles',
  'executeViewStructure',
  'LocalRipgrepQuerySchema',
  'LocalFindFilesQuerySchema',
  'LocalViewStructureQuerySchema',
  'RipgrepQuerySchema',
  'FindFilesQuerySchema',
  'ViewStructureQuerySchema',
] as const;

async function moduleImports(relativePath: string): Promise<string[]> {
  const file = path.join(SOURCE_ROOT, relativePath);
  const source = await readFile(file, 'utf8');
  const parsed = ts.createSourceFile(
    file,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS
  );

  return parsed.statements.flatMap(statement => {
    if (
      !ts.isImportDeclaration(statement) ||
      !ts.isStringLiteral(statement.moduleSpecifier)
    ) {
      return [];
    }
    return [statement.moduleSpecifier.text];
  });
}

describe('localSearch architecture boundary', () => {
  it('publishes only the unified local discovery contract', () => {
    expect(publicApi).toHaveProperty('executeLocalSearch');
    expect(publicApi).toHaveProperty('LocalSearchQuerySchema');

    for (const legacyExport of LEGACY_EXPORTS) {
      expect(publicApi, legacyExport).not.toHaveProperty(legacyExport);
    }

    const publicTools = new Map(
      DIRECT_TOOL_DISCOVERY_DEFINITIONS.map(tool => [tool.name, tool])
    );
    expect(publicTools.has('localSearch')).toBe(true);
    for (const legacyName of LEGACY_PUBLIC_NAMES) {
      expect(publicTools.has(legacyName)).toBe(false);
    }
  });

  it('adapts the three internal engines without nesting their public bulk wrappers', async () => {
    const imports = await moduleImports('tools/local_search/execution.ts');

    expect(imports).toEqual(
      expect.arrayContaining([
        '../local_ripgrep/searchContentRipgrep.js',
        '../local_find_files/findFiles.js',
        '../local_view_structure/local_view_structure.js',
      ])
    );
    expect(imports).not.toEqual(
      expect.arrayContaining([
        '../local_ripgrep/execution.js',
        '../local_find_files/execution.js',
        '../local_view_structure/execution.js',
      ])
    );
  });
});
