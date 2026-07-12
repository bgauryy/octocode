import { existsSync, rmSync } from 'node:fs';
import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { EXIT } from '../exit-codes.js';
import { c, dim } from '../../utils/colors.js';
import {
  formatBytes,
  getDirectorySizeBytes,
} from '@octocodeai/octocode-tools-core/fs-utils';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import {
  materializeRemoteForCli,
  type RemoteMaterialization,
  type RemoteMaterializationKind,
} from '../remote-local.js';

const DEPTH_VALUES = new Set(['file', 'tree', 'clone']);

function printUsage(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.error(`\n  ${c('red', '✗')} ${message}`);
    console.error(
      `\n  ${dim('Examples:')}\n` +
        `    cache fetch vercel/next.js README.md --depth file\n` +
        `    cache fetch vercel/next.js packages/next --depth tree\n` +
        `    cache fetch vercel/next.js --depth clone\n` +
        `    cache status\n` +
        `\n  ${dim('Flow:')}\n` +
        `    cache fetch checks existing tmp materialization first; use --force-refresh to bypass it.\n` +
        `    Use the returned localPath with search, search --tree, and search --op for LSP semantics.\n`
    );
  }
  process.exitCode = EXIT.USAGE;
}

/** Infer depth from the requested path when --depth is not given:
 *  - no path → clone the full repo
 *  - path with a file extension (last segment contains '.') → single file
 *  - path without extension → treat as a directory subtree
 */
function inferDepth(requestedPath: string): 'file' | 'tree' | 'clone' {
  if (!requestedPath) return 'clone';
  const last = requestedPath.split('/').pop() ?? '';
  return last.includes('.') ? 'file' : 'tree';
}

function depthToKind(depth: string): RemoteMaterializationKind {
  if (depth === 'file') return 'file';
  if (depth === 'tree') return 'tree';
  return 'repo';
}

function renderMaterialization(result: RemoteMaterialization): void {
  console.log();
  console.log(
    `  ${c('green', '✓')} Saved ${result.owner}/${result.repo} locally`
  );
  console.log(`  ${dim('localPath:')} ${c('cyan', result.localPath)}`);
  console.log(`  ${dim('repoRoot:')}  ${c('cyan', result.repoRoot)}`);
  if (result.branch) console.log(`  ${dim('ref:')}       ${result.branch}`);
  if (result.requestedPath) {
    console.log(`  ${dim('path:')}      ${result.requestedPath}`);
  }
  console.log();
  const { location } = result;
  console.log('location:');
  console.log(`  ${dim('kind:')}      ${location.kind}`);
  console.log(`  ${dim('localPath:')} ${location.localPath}`);
  if (location.repoRoot) {
    console.log(`  ${dim('repoRoot:')}  ${location.repoRoot}`);
  }
  if (location.requestedPath) {
    console.log(`  ${dim('requestedPath:')} ${location.requestedPath}`);
  }
  if (location.source) {
    console.log(`  ${dim('source:')}    ${location.source}`);
  }
  if (location.resolvedBranch) {
    console.log(`  ${dim('resolvedBranch:')} ${location.resolvedBranch}`);
  }
  if (location.cached !== undefined) {
    console.log(`  ${dim('cached:')}    ${location.cached}`);
  }
  if (location.complete !== undefined) {
    console.log(`  ${dim('complete:')}  ${location.complete}`);
  }
  if (location.verified !== undefined) {
    console.log(`  ${dim('verified:')}  ${location.verified}`);
  }
  if (location.commitSha) {
    console.log(`  ${dim('commitSha:')} ${location.commitSha}`);
  }
  if (location.hasSubdirectories) {
    console.log(`  ${dim('hasSubdirectories:')} true`);
  }
  if (
    location.skippedSummary &&
    Object.keys(location.skippedSummary).length > 0
  ) {
    console.log(
      `  ${dim('skippedSummary:')} ${JSON.stringify(location.skippedSummary)}`
    );
  }
  console.log();
}

function printStatus(jsonOutput: boolean): void {
  const cloneBytes = getDirectorySizeBytes(paths.clone);
  const treeBytes = getDirectorySizeBytes(paths.tree);
  const tmpBytes = cloneBytes + treeBytes;
  const payload = {
    home: paths.home,
    tmp: {
      path: paths.tmp,
      exists: existsSync(paths.tmp),
      sizeBytes: tmpBytes,
      sizeFormatted: formatBytes(tmpBytes),
    },
    clone: {
      path: paths.clone,
      exists: existsSync(paths.clone),
      sizeBytes: cloneBytes,
      sizeFormatted: formatBytes(cloneBytes),
    },
    tree: {
      path: paths.tree,
      exists: existsSync(paths.tree),
      sizeBytes: treeBytes,
      sizeFormatted: formatBytes(treeBytes),
    },
  };

  if (jsonOutput) {
    console.log(JSON.stringify(payload, null, 2));
    return;
  }

  console.log();
  console.log(`  ${dim('Octocode home:')} ${payload.home}`);
  console.log(
    `  ${dim('tmp cache:')}    ${payload.tmp.path} (${payload.tmp.sizeFormatted})`
  );
  console.log(
    `  ${dim('clone cache:')}  ${payload.clone.path} (${payload.clone.sizeFormatted})`
  );
  console.log(
    `  ${dim('tree cache:')}   ${payload.tree.path} (${payload.tree.sizeFormatted})`
  );
  console.log();
}

function clearCachePaths(
  jsonOutput: boolean,
  selections: {
    clone: boolean;
    tree: boolean;
    all: boolean;
  }
): void {
  const cleared: Record<string, string> = {};
  const remove = (key: string, dir: string): void => {
    if (existsSync(dir)) {
      rmSync(dir, { recursive: true, force: true });
    }
    cleared[key] = dir;
  };

  if (selections.all) {
    remove('tmp', paths.tmp);
  } else {
    if (selections.clone) remove('clone', paths.clone);
    if (selections.tree) remove('tree', paths.tree);
  }

  if (jsonOutput) {
    console.log(
      JSON.stringify({
        success: true,
        cleared,
      })
    );
    return;
  }

  console.log();
  for (const [key, dir] of Object.entries(cleared)) {
    console.log(`  ${c('green', '✓')} Cleared ${key} cache: ${dir}`);
  }
  console.log();
}

export const cacheCommand: CLICommand = {
  name: 'cache',
  options: [
    { name: 'depth', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'force-refresh' },
    { name: 'clone' },
    { name: 'repos' },
    { name: 'tree' },
    { name: 'all' },
    { name: 'json' },
  ],
  handler: async args => {
    const subcommand = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');

    if (subcommand === 'status') {
      printStatus(jsonOutput);
      return;
    }

    if (subcommand === 'clear') {
      const selections = {
        clone: getBool(args.options, 'clone') || getBool(args.options, 'repos'),
        tree: getBool(args.options, 'tree'),
        all: getBool(args.options, 'all'),
      };
      if (!selections.clone && !selections.tree && !selections.all) {
        printUsage(
          'cache clear requires --clone, --tree, --repos, or --all.',
          jsonOutput
        );
        return;
      }
      clearCachePaths(jsonOutput, selections);
      return;
    }

    if (subcommand !== 'fetch') {
      printUsage('Use cache fetch, cache status, or cache clear.', jsonOutput);
      return;
    }

    const repoRef = args.args[1] ?? '';
    const requestedPath = args.args[2] ?? '';
    const depth = getString(args.options, 'depth') || inferDepth(requestedPath);
    if (!repoRef) {
      printUsage('cache fetch requires owner/repo[@ref].', jsonOutput);
      return;
    }
    if (!DEPTH_VALUES.has(depth)) {
      printUsage('--depth must be file, tree, or clone.', jsonOutput);
      return;
    }

    try {
      const result = await materializeRemoteForCli({
        repoRef,
        path: requestedPath || undefined,
        branch: getString(args.options, 'branch') || undefined,
        forceRefresh: getBool(args.options, 'force-refresh') || undefined,
        kind: depthToKind(depth),
      });

      if (jsonOutput) {
        console.log(
          JSON.stringify(
            {
              success: true,
              ...result,
            },
            null,
            2
          )
        );
        return;
      }

      renderMaterialization(result);
    } catch (caught) {
      let message = caught instanceof Error ? caught.message : String(caught);
      // A directory can't be fetched as a single file. The raw tool points at
      // ghViewRepoStructure, which only lists — steer the user to the cache
      // command's own subtree mode (and clone), which actually land on disk.
      if (/is a directory/i.test(message) && requestedPath) {
        message =
          `"${requestedPath}" is a directory, not a file. ` +
          `Cache the subtree with: cache fetch ${repoRef} ${requestedPath} --depth tree ` +
          `(or clone ${repoRef}/${requestedPath} for a working copy).`;
      }
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            error: message,
          })
        );
      } else {
        console.error(`\n  ${c('red', '✗')} ${message}\n`);
      }
      process.exitCode = EXIT.TOOL;
    }
  },
};
