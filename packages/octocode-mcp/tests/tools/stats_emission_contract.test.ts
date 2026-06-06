import { readFile } from 'fs/promises';
import { describe, expect, it } from 'vitest';

const ROOT = process.cwd();

const registeredTools = [
  {
    name: 'githubSearchCode',
    executionFiles: ['src/tools/github_search_code/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'githubGetFileContent',
    executionFiles: ['src/tools/github_fetch_content/execution.ts'],
    rawEvidence: [
      /rawResponse:\s*providerResult\.response\.rawResponseChars/,
      /rawResponse:\s*result\.totalSize\s*\?\?\s*countSerializedChars\(result\)/,
    ],
  },
  {
    name: 'githubViewRepoStructure',
    executionFiles: ['src/tools/github_view_repo_structure/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'githubSearchRepositories',
    executionFiles: ['src/tools/github_search_repos/execution.ts'],
    rawEvidence: [/rawResponse:\s*sumVariantRawResponseChars\(/],
  },
  {
    name: 'githubSearchPullRequests',
    executionFiles: ['src/tools/github_search_pull_requests/execution.ts'],
    rawEvidence: [/rawResponse:\s*providerResult\.response\.rawResponseChars/],
  },
  {
    name: 'packageSearch',
    executionFiles: ['src/tools/package_search/execution.ts'],
    rawEvidence: [/rawResponse:\s*apiResult/],
  },
  {
    name: 'githubCloneRepo',
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
      /rawResponse:\s*result\.stdout\.length\s*\+\s*result\.stderr\.length/,
      /attachRawResponseChars\([\s\S]*countSerializedChars\(entries\)/,
    ],
  },
  {
    name: 'localFindFiles',
    executionFiles: [
      'src/tools/local_find_files/execution.ts',
      'src/tools/local_find_files/findFiles.ts',
    ],
    rawEvidence: [
      /rawResponse:\s*result\.stdout\.length\s*\+\s*result\.stderr\.length/,
      /attachRawResponseChars\([\s\S]*result\.stdout\.length/,
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
    name: 'lspGotoDefinition',
    executionFiles: ['src/tools/lsp_goto_definition/execution.ts'],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*content\.length\s*\+\s*countSerializedChars\(result\)/,
    ],
  },
  {
    name: 'lspFindReferences',
    executionFiles: [
      'src/tools/lsp_find_references/execution.ts',
      'src/tools/lsp_find_references/lsp_find_references.ts',
    ],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*content\.length\s*\+\s*countSerializedChars\(lspResult\)/,
    ],
  },
  {
    name: 'lspCallHierarchy',
    executionFiles: [
      'src/tools/lsp_call_hierarchy/execution.ts',
      'src/tools/lsp_call_hierarchy/callHierarchy.ts',
    ],
    rawEvidence: [
      /attachRawResponseChars\([\s\S]*content\.length\s*\+\s*countSerializedChars\(result\)/,
    ],
  },
] as const;

async function readProjectFile(relativePath: string): Promise<string> {
  return readFile(`${ROOT}/${relativePath}`, 'utf-8');
}

describe('tool stats emission contract', () => {
  it('covers every registered tool from the catalog', async () => {
    const toolConfig = await readProjectFile('src/tools/toolConfig.ts');
    const catalogNames = [
      ...toolConfig.matchAll(/const\s+([A-Z_]+)\s*=\s*createTool\(/g),
    ].map(match => match[1]);

    expect(catalogNames).toHaveLength(14);
    expect(registeredTools.map(tool => tool.name)).toHaveLength(
      catalogNames.length
    );
  });

  it('records final sent response length once per bulk tool invocation', async () => {
    const bulk = await readProjectFile('src/utils/response/bulk.ts');

    expect(bulk).toMatch(/createResponseFormat\(/);
    expect(bulk).toMatch(
      /recordBulkCharSavings\(config\.toolName, results, errors, text\.length\);/
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
      '../octocode-security-utils/src/withSecurityValidation.ts'
    );

    expect(indexSource).toMatch(/configureSecurity\(\{[\s\S]*logToolCall/);
    expect(indexSource).toMatch(/configureSecurity\(\{[\s\S]*isLocalTool/);
    expect(securitySource).toMatch(
      /withSecurityValidation[\s\S]*handleBulk\(toolName, sanitizedParams\)/
    );
    expect(securitySource).toMatch(
      /withBasicSecurityValidation[\s\S]*_deps\.isLocalTool\?\.\(toolName\)[\s\S]*handleBulk\(/
    );
  });
});
