import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { resolveRef, isGithubRef, refLabel } from '../routing.js';
import { c, bold, dim } from '../../utils/colors.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';

interface TreeEntry {
  files?: string[];
  folders?: string[];
  summary?: string;
}

interface StructureResult {
  results?: Array<{
    data?: {
      path?: string;
      files?: string[];
      folders?: string[];
      summary?: string;
      structure?: Record<string, TreeEntry>;
    };
  }>;
}

type LocalStructureResult = StructureResult;
type GithubStructureResult = StructureResult;

async function fetchLocalTree(
  dirPath: string,
  depth?: number
): Promise<LocalStructureResult> {
  const result = await executeDirectTool('localViewStructure', {
    queries: [
      {
        path: dirPath,
        depth,
        mainResearchGoal: 'View directory structure',
        researchGoal: 'Get local directory tree',
        reasoning: 'CLI tree command',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    throw new Error(`Local structure error: ${errText}`);
  }

  return result.structuredContent as LocalStructureResult;
}

async function fetchGithubTree(
  owner: string,
  repo: string,
  subpath: string,
  branch?: string,
  depth?: number
): Promise<GithubStructureResult> {
  const result = await executeDirectTool('ghViewRepoStructure', {
    queries: [
      {
        owner,
        repo,
        path: subpath || '',
        branch,
        depth,
        mainResearchGoal: 'View repository structure',
        researchGoal: 'Get GitHub directory tree',
        reasoning: 'CLI tree command',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    if (/401|403|auth/i.test(errText)) {
      throw new Error(
        `GitHub auth error: ${errText}. Set GITHUB_TOKEN, OCTOCODE_TOKEN, or GH_TOKEN.`
      );
    }
    if (/404|not found/i.test(errText)) {
      throw new Error(`Not found on GitHub: ${owner}/${repo}/${subpath}`);
    }
    throw new Error(`GitHub API error: ${errText}`);
  }

  return result.structuredContent as GithubStructureResult;
}

function renderTree(data: Record<string, unknown> | undefined): string {
  if (!data) return '(empty)';
  const lines: string[] = [];

  if ('structure' in data && data.structure) {
    for (const [dirPath, entry] of Object.entries(
      data.structure as Record<string, TreeEntry>
    )) {
      lines.push(bold(dirPath) + '/');
      (entry.folders ?? []).forEach(f =>
        lines.push(`  ${c('cyan', '📁')} ${f}/`)
      );
      (entry.files ?? []).forEach(f => lines.push(`  ${c('green', '·')} ${f}`));
    }
    return lines.join('\n');
  }

  if ('path' in data && data.path) lines.push(bold(data.path as string) + '/');
  ((data.folders as string[] | undefined) ?? []).forEach(f =>
    lines.push(`  ${c('cyan', '📁')} ${f}/`)
  );
  ((data.files as string[] | undefined) ?? []).forEach(f =>
    lines.push(`  ${c('green', '·')} ${f}`)
  );
  if ('summary' in data && data.summary)
    lines.push(`\n  ${dim(data.summary as string)}`);
  return lines.join('\n');
}

export const treeCommand: CLICommand = {
  name: 'tree',
  description:
    'View directory structure — works for local paths and GitHub repositories',
  usage:
    'octocode tree <path|github-ref> [--depth <n>] [--branch <ref>] [--json]',
  options: [
    {
      name: 'depth',
      hasValue: true,
      description:
        'Directory depth (default: 2 for GitHub, server default for local)',
    },
    {
      name: 'branch',
      hasValue: true,
      description: 'Branch / ref for GitHub paths',
    },
    {
      name: 'json',
      description: 'Output raw JSON structure',
    },
  ],
  handler: async args => {
    const { options } = args;
    const target = args.args[0] ?? '';
    const branchOverride = getString(options, 'branch');
    const rawDepth = getString(options, 'depth');
    const depthExplicit = rawDepth ? parseInt(rawDepth, 10) : undefined;
    const jsonOutput = getBool(options, 'json');

    if (!target) {
      const err = 'Provide a path or GitHub reference.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${err}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    octocode tree src/\n` +
            `    octocode tree bgauryy/octocode-mcp/packages\n` +
            `    octocode tree bgauryy/octocode-mcp --depth 2\n`
        );
      }
      process.exitCode = 1;
      return;
    }

    const ref = resolveRef(target, branchOverride || undefined);
    const label = refLabel(ref);

    if (!jsonOutput) {
      process.stderr.write(`  ${dim(`Loading ${label} ...`)}\n`);
    }

    try {
      let structured: LocalStructureResult | GithubStructureResult;

      if (isGithubRef(ref)) {
        structured = await fetchGithubTree(
          ref.owner,
          ref.repo,
          ref.subpath,
          ref.branch,
          depthExplicit ?? 2
        );
      } else {
        structured = await fetchLocalTree(ref.path, depthExplicit);
      }

      if (jsonOutput) {
        console.log(JSON.stringify(structured, null, 2));
        return;
      }

      const data = structured?.results?.[0]?.data;
      console.log('\n' + renderTree(data) + '\n');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${msg}\n`);
      }
      process.exitCode = 1;
    }
  },
};
