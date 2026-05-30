import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';

type NpmPackageQuery = z.infer<typeof NpmPackageQuerySchema>;
import {
  PackageSearchBulkQueryLocalSchema,
  PackageSearchOutputLocalSchema,
} from '../../scheme/remoteSchemaOverlay.js';
import { checkNpmAvailability } from '../../utils/exec/npm.js';
import { checkNpmRegistryReachable } from '../../utils/package/npm.js';
import { searchPackages } from './execution.js';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerPackageSearchTool =
  createRemoteToolRegistration<NpmPackageQuery>({
    name: TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    inputSchema: PackageSearchBulkQueryLocalSchema,
    outputSchema: PackageSearchOutputLocalSchema,
    executionFn: searchPackages,
    registrationGuard: async () => {
      if (!(await checkNpmAvailability(10000))) return false;
      if (!(await checkNpmRegistryReachable())) return false;
      return true;
    },
    annotations: {
      readOnlyHint: true,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
