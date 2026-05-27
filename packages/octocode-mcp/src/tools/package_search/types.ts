import type { z } from 'zod/v4';
import type { NpmPackageQuerySchema } from '@octocodeai/octocode-core';
import type {
  MinimalPackageResult as CommonMinimalPackageResult,
  NpmPackageResult as CommonNpmPackageResult,
  PackageResult as CommonPackageResult,
  PackageSearchAPIResult as CommonPackageSearchAPIResult,
  PackageSearchError as CommonPackageSearchError,
  DeprecationInfo as CommonDeprecationInfo,
} from '../../utils/package/common.js';

export type NpmPackageSearchQuery = z.infer<typeof NpmPackageQuerySchema>;

export type MinimalPackageResult = CommonMinimalPackageResult;

export type NpmPackageResult = CommonNpmPackageResult;

export type PackageResult = CommonPackageResult;

export type DeprecationInfo = CommonDeprecationInfo;

export type PackageSearchAPIResult = CommonPackageSearchAPIResult;

export type PackageSearchError = CommonPackageSearchError;
