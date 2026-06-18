import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';
import { ALL_TOOLS } from '../../src/tools/toolConfig.js';

import { resolve } from 'path';
const ROOT = process.cwd();
const CORE_ROOT = resolve(ROOT, '../octocode-tools-core');

const registeredTools = [
  {
    name: 'ghSearchCode',
    executionFiles: ['src/tools/github_search_code/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'ghGetFileContent',
    executionFiles: ['src/tools/github_fetch_content/execution.ts'],
    rawEvidence: [
      /rawResponse:\s*providerResult\.response\.rawResponseChars/,
      /rawResponse:\s*result\.totalSize\s*\?\?\s*countSerializedChars\(result\)/,
    ],
  },
  {
    name: 'ghViewRepoStructure',
    executionFiles: ['src/tools/github_view_repo_structure/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'ghSearchRepos',
    executionFiles: ['src/tools/github_search_repos/execution.ts'],
    rawEvidence: [/rawResponse:\s*sumVariantRawResponseChars\(/],
  },
  {
    name: 'ghHistoryResearch',
    executionFiles: ['src/tools/github_search_pull_requests/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'npmSearch',
    executionFiles: ['src/tools/package_search/execution.ts'],
    rawEvidence: [/rawResponse:\s*apiResult/],
  },
  {
    name: 'ghCloneRepo',
    executionFiles: ['src/tools/github_clone_repo/execution.ts'],
    rawEvidence: [/rawResponse:\s*getDirectorySizeBytes\(result\.localPath\)/],
  },
  {
    name: 'localSearchCode',
    executionFiles: [
      'src/tools/local_ripgrep/execution.ts',
      'src/tools/local_ripgrep/ripgrepExecutor.ts',
    ],
    rawEvidence: [
      /rawResponse:\s*result\.stdout\.length\s*\+\s*result\.stderr\.length/,
      /attachRawResponseChars\(searchResult,\s*result\.stdout\.length\)/,
    ],
  },
  {
    name: 'localViewStructure',
    executionFiles: [
      'src/tools/local_view_structure/execution.ts',
      'src/tools/local_view_structure/local_view_structure.ts',
    ],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*nativeResult\.entries\.reduce\(\s*\(sum, entry\) => sum \+ entry\.path\.length/,
    ],
  },
  {
    name: 'localFindFiles',
    executionFiles: [
      'src/tools/local_find_files/execution.ts',
      'src/tools/local_find_files/findFiles.ts',
    ],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*nativeResult\.entries\.reduce\(\s*\(sum, entry\) => sum \+ entry\.path\.length/,
    ],
  },
  {
    name: 'localGetFileContent',
    executionFiles: [
      'src/tools/local_fetch_content/execution.ts',
      'src/tools/local_fetch_content/fetchContent.ts',
    ],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*fileSizeBytes/,
      /attachRawResponseChars\([\s\S]*content\.length/,
    ],
  },
  {
    name: 'lspGetSemantics',
    executionFiles: ['src/tools/lsp/semantic_content/execution.ts'],
    rawEvidence: [
      /attachRawResponseChars\(result,\s*countSerializedChars\(result\)\)/,
    ],
  },
  {
    name: 'localBinaryInspect',
    executionFiles: ['src/tools/local_binary_inspect/execution.ts'],
    rawEvidence: [/attachRawResponseChars\(result/],
  },
] as const;

async function readProjectFile(relativePath: string): Promise<string> {
  const isMcpOnly =
    relativePath.startsWith('src/index.ts') ||
    relativePath.startsWith('src/public') ||
    relativePath.startsWith('src/tools/toolsManager') ||
    relativePath.startsWith('src/tools/toolConfig') ||
    relativePath.startsWith('src/tools/toolFilters') ||
    relativePath.startsWith('src/utils/core/logger') ||
    relativePath.startsWith('src/utils/secureServer');
  const root = isMcpOnly ? ROOT : CORE_ROOT;
  return readFile(`${root}/${relativePath}`, 'utf-8');
}

describe('tool stats emission contract', () => {
  it('covers every registered tool from the catalog', async () => {
    const catalogNames = ALL_TOOLS.map(tool => tool.name).sort();
    const coveredNames = registeredTools.map(tool => tool.name).sort();

    expect(catalogNames).toHaveLength(13);
    expect(coveredNames).toEqual(catalogNames);
  });

  it('records final sent response length once per bulk tool invocation', async () => {
    const bulk = await readProjectFile('src/utils/response/bulk.ts');

    expect(bulk).toMatch(/createResponseFormat\(/);
    expect(bulk).toMatch(
      /recordBulkCharSavings\(\s*config\.toolName,\s*results,\s*errors,\s*paginated\.text\.length\s*\);/
    );
    expect(bulk).toMatch(
      /incrementToolCharSavings\(toolName, rawChars, responseChars\);/
    );
  });

  for (const tool of registeredTools) {
    it(`${tool.name} routes through bulk telemetry and attaches raw source metrics`, async () => {
      const sources = await Promise.all(
        tool.executionFiles.map(readProjectFile)
      );
      const combinedSource = sources.join('\n');

      expect(
        combinedSource,
        `${tool.name} must route through executeBulkOperation so final sent size is measured`
      ).toMatch(/executeBulkOperation\(/);

      for (const evidence of tool.rawEvidence) {
        expect(
          combinedSource,
          `${tool.name} missing raw metric evidence: ${evidence}`
        ).toMatch(evidence);
      }
    });
  }

  it('security wrappers emit tool-call state for both remote and local tools', async () => {
    const indexSource = await readProjectFile('src/index.ts');
    const securitySource = await readProjectFile(
      '../octocode-security/src/withSecurityValidation.ts'
    );

    expect(indexSource).toMatch(/configureSecurity\(\{[\s\S]*logToolCall/);
    expect(indexSource).not.toMatch(/configureSecurity\(\{[\s\S]*isLocalTool/);
    expect(securitySource).toMatch(
      /runSecure[\s\S]*handleBulk\(toolName, sanitizedParams\)/
    );
    expect(securitySource).toMatch(/withSecurityValidation[\s\S]*runSecure\(/);
    expect(securitySource).toMatch(
      /withBasicSecurityValidation[\s\S]*runSecure\(/
    );
  });
});
