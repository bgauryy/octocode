import type { NpmSearchAPIResult, NpmSearchError } from '../types.js';
import { NpmViewResultSchema } from '../schemas.js';
import {
  fetchNpmRegistryJson,
  type NpmRegistryContext,
  type NpmViewResult,
} from './npmRegistry.js';
import {
  countRawPayloadChars,
  encodeRegistryPackageName,
  mapToResult,
} from './npmMappers.js';

export async function fetchNpmPackage(
  packageName: string,
  fetchMetadata: boolean,
  context: NpmRegistryContext
): Promise<NpmSearchAPIResult | NpmSearchError> {
  try {
    const raw = await fetchNpmRegistryJson(
      context,
      `${encodeRegistryPackageName(packageName)}/latest`
    );
    const rawResponseChars = countRawPayloadChars(raw);
    const validation = NpmViewResultSchema.safeParse(raw);
    if (!validation.success)
      return {
        error: 'Invalid npm registry package response.',
        rawResponseChars,
      };
    if (validation.data.name !== packageName)
      return {
        error: 'npm registry returned a different package name.',
        rawResponseChars,
      };
    return {
      packages: [
        mapToResult(
          validation.data as NpmViewResult,
          fetchMetadata,
          'registry'
        ),
      ],
      totalFound: 1,
      rawResponseChars,
    };
  } catch (error) {
    const message =
      error instanceof Error ? error.message : 'npm package request failed.';
    if (message.includes('404 Not Found'))
      return { packages: [], totalFound: 0, rawResponseChars: 0 };
    return { error: message };
  }
}
