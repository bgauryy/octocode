/**
 * Response-state hints for githubViewRepoStructure.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/github_view_repo_structure/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;

    // Surface feature-flag / mode / config files at the top of the listing.
    // Benchmark Q8 lost a point because Recoil_ReactMode.js — which gates the
    // hook implementation behind a feature flag — was never inspected. If a
    // dir contains *Mode/*Config/*Flag files, the agent should read those
    // before assuming the search result is canonical.
    const flagFiles = Array.isArray(c.flagFiles)
      ? (c.flagFiles as unknown[]).filter(
          (f): f is string => typeof f === 'string'
        )
      : [];
    if (flagFiles.length > 0) {
      const shown = flagFiles.slice(0, 3).join(', ');
      const more =
        flagFiles.length > 3 ? `, +${flagFiles.length - 3} more` : '';
      out.push(
        `Flag/Mode/Config files present (${shown}${more}). Read these FIRST — they often gate the implementation a direct search misses.`
      );
    }

    if (ctx.entryCount && ctx.entryCount > 50) {
      out.push(
        `Large directory (${ctx.entryCount} entries). Narrow with path or depth=1.`
      );
    }
    return out;
  },

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' && c.path ? c.path : undefined;
    const depth = typeof c.depth === 'number' ? c.depth : undefined;
    const branch =
      typeof c.branch === 'string' && c.branch ? c.branch : undefined;
    const where = path ? `'${path}'` : 'the repository root';
    const onBranch = branch ? ` on branch '${branch}'` : '';
    if (depth === 1) {
      return [
        `Empty listing for ${where}${onBranch}. Try depth=2 or a different path.`,
      ];
    }
    return [
      `Empty listing for ${where}${onBranch}. Verify the path with a parent directory or different branch.`,
    ];
  },

  error: (_ctx: HintContext = {}) => [],
};
