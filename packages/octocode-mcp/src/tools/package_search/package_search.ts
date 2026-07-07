import type { z } from 'zod';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  TOOL_NAMES,
  NpmSearchBulkQueryLocalSchema,
  NpmSearchOutputLocalSchema,
  searchPackages,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

type NpmPackageQuery = z.input<typeof NpmPackageQuerySchema>;
type NpmSearchQuery = Omit<NpmPackageQuery, 'ecosystem'> & {
  ecosystem?: 'npm';
};

export const registerNpmSearchTool =
  createRemoteToolRegistration<NpmSearchQuery>({
    name: TOOL_NAMES.PACKAGE_SEARCH,
    title: 'Package Search',
    inputSchema: NpmSearchBulkQueryLocalSchema,
    outputSchema: NpmSearchOutputLocalSchema,
    executionFn: searchPackages,
    annotations: {
      readOnlyHint: true,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: true,
    },
  });
