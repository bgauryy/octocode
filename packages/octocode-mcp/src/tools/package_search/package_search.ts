import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { z } from 'zod';
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

export const registerPackageSearchTool =
  createRemoteToolRegistration<PackageSearchQuery>({
    name: TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    inputSchema: PackageSearchBulkQueryLocalSchema,
    outputSchema: PackageSearchOutputLocalSchema,
    executionFn: searchPackages,
    annotations: {
      readOnlyHint: true,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
