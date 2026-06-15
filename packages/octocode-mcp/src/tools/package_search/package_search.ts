import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  TOOL_NAMES,
  PackageSearchBulkQueryLocalSchema,
  PackageSearchOutputLocalSchema,
  searchPackages,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

type NpmPackageQuery = z.input<typeof NpmPackageQuerySchema>;
type PackageSearchQuery = Omit<NpmPackageQuery, 'ecosystem'> & {
  ecosystem?: 'npm';
};

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
