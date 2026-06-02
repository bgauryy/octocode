import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmPackageQuery = z.infer<typeof NpmPackageQuerySchema>;
type PackageSearchQuery = Omit<NpmPackageQuery, 'ecosystem'> & {
  ecosystem?: 'npm';
};
import {
  PackageSearchBulkQueryLocalSchema,
  PackageSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchPackages } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

/**
 * Correct the agent-facing packageSearch description.
 *
 * The upstream metadata (octocode-core default.json) still documents the legacy
 * `searchLimit` knob, but this server's overlay removed it in favour of the
 * cross-tool `itemsPerPage` (see remoteSchemaOverlay.ts — "ONE result-count
 * knob: itemsPerPage"). Left as-is, the description tells the agent to pass a
 * field the schema strips, so the cap silently no-ops. `searchLimit` maps 1:1
 * to `itemsPerPage`, so a literal rename keeps every example correct.
 */
export function describePackageSearch(base: string): string {
  const corrected = base.replaceAll('searchLimit', 'itemsPerPage');
  return `${corrected}
<when>Use packageSearch when you know a registry package name and need the canonical repository URL; use githubSearchRepositories for broad repo discovery.</when>`;
}

export const registerPackageSearchTool =
  createRemoteToolRegistration<PackageSearchQuery>({
    name: TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    inputSchema: PackageSearchBulkQueryLocalSchema,
    outputSchema: PackageSearchOutputLocalSchema,
    executionFn: searchPackages,
    describe: describePackageSearch,
    // No registrationGuard: packageSearch is ALWAYS registered. npm/registry
    // reachability is a per-CALL concern, handled gracefully by searchPackages
    // (try/catch → structured error result). A startup probe would otherwise
    // make the tool silently vanish on a transient blip / offline startup and
    // add npm-probe latency to every server init. (#T4 — guard removed)
    annotations: {
      readOnlyHint: true,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
