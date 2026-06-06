import { Router, type Request, type Response, type NextFunction } from 'express';
import { packageSearch } from '../index.js';
import { parseAndValidate } from '../middleware/queryParser.js';
import { packageSearchSchema } from '../validation/index.js';
import { ResearchResponse } from '../utils/responseBuilder.js';
import { parseToolResponse } from '../utils/responseParser.js';
import { withPackageResilience } from '../utils/resilience.js';

import { safeString, safeArray } from '../utils/responseFactory.js';
import { isObject, hasProperty, hasStringProperty } from '../types/guards.js';

export const packageRoutes = Router();

packageRoutes.get(
  '/packageSearch',
  async (req: Request, res: Response, next: NextFunction) => {
    try {
      const queries = parseAndValidate(
        req.query as Record<string, unknown>,
        packageSearchSchema
      );
      type PackageSearchParams = Parameters<typeof packageSearch>[0];
      const rawResult = await withPackageResilience(
        () => packageSearch({ queries } as PackageSearchParams),
        'packageSearch'
      );
      const { data, isError, hints, research } = parseToolResponse(rawResult);

      const packages = extractPackages(data);
      const query = queries[0] as Record<string, unknown>;
      const registry = query.ecosystem === 'python' ? 'pypi' : 'npm';

      const response = ResearchResponse.packageSearch({
        packages,
        registry,
        query: safeString(query, 'name'),
        mcpHints: hints,
        research,
      });

      res.status(isError ? 500 : 200).json(response);
    } catch (error) {
      next(error);
    }
  }
);

interface PackageInfo {
  name: string;
  version?: string;
  description?: string;
  repository?: string;
}

function extractRepositoryUrl(pkg: Record<string, unknown>): string | undefined {
  if (hasStringProperty(pkg, 'repository')) return pkg.repository;
  if (hasStringProperty(pkg, 'repoUrl')) return pkg.repoUrl;
  if (hasProperty(pkg, 'repository') && isObject(pkg.repository)) {
    const repo = pkg.repository;
    if (hasStringProperty(repo, 'url')) return repo.url;
  }
  return undefined;
}

function extractPypiRepo(pkg: Record<string, unknown>): string | undefined {
  if (hasStringProperty(pkg, 'homepage')) return pkg.homepage;
  if (hasStringProperty(pkg, 'project_url')) return pkg.project_url;
  return undefined;
}

function toPackageInfo(pkg: unknown, repoFn: (p: Record<string, unknown>) => string | undefined): PackageInfo {
  if (!isObject(pkg)) return { name: '' };
  return {
    name: safeString(pkg, 'name') || safeString(pkg, 'path'),
    version: hasStringProperty(pkg, 'version') ? pkg.version : undefined,
    description: hasStringProperty(pkg, 'description') ? pkg.description : undefined,
    repository: repoFn(pkg),
  };
}

function getPackageArray(data: Record<string, unknown>): { items: unknown[]; source: 'npm' | 'pypi' | 'generic' | 'fallback' } {
  if (hasProperty(data, 'npmResults') && Array.isArray(data.npmResults))
    return { items: data.npmResults, source: 'npm' };
  if (hasProperty(data, 'pypiResults') && Array.isArray(data.pypiResults))
    return { items: data.pypiResults, source: 'pypi' };
  if (hasProperty(data, 'packages') && Array.isArray(data.packages))
    return { items: data.packages, source: 'generic' };
  return { items: safeArray<Record<string, unknown>>(data, 'results'), source: 'fallback' };
}

function extractPackages(data: Record<string, unknown>): PackageInfo[] {
  const { items, source } = getPackageArray(data);
  const repoFn = source === 'pypi' ? extractPypiRepo : extractRepositoryUrl;
  return items.map((pkg) => toPackageInfo(pkg, repoFn));
}
