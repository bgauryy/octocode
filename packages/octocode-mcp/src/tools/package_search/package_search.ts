import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmPackageQuery = z.infer<typeof NpmPackageQuerySchema>;
import {
  PackageSearchBulkQueryLocalSchema,
  PackageSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { searchPackages } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerPackageSearchTool =
  createRemoteToolRegistration<NpmPackageQuery>({
    name: TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    inputSchema: PackageSearchBulkQueryLocalSchema,
    outputSchema: PackageSearchOutputLocalSchema,
    executionFn: searchPackages,
    describe: base => `${base}
<when>Use packageSearch when you know a registry package name and need the canonical repository URL; use githubSearchRepositories for broad repo discovery.</when>`,
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
