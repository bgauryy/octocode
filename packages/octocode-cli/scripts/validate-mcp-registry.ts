#!/usr/bin/env npx tsx

import {
  MCP_REGISTRY,
  type MCPRegistryEntry,
} from '../src/configs/mcp-registry.js';
import {
  buildValidationJsonSummary,
  checkGitHubRepository,
  formatRelativeTime,
  hasBlockingValidationFailures,
  printRateLimitTip,
  printReportHeader,
  printSectionHeader,
  printSummary,
  printValidatorBanner,
  resolveValidatorToken,
  splitValidationResults,
  topByStars,
  writeValidationProgress,
  type BaseValidationResult,
} from './validation-report-helpers.js';

interface ValidationResult extends BaseValidationResult {
  repository: string;
  npmPackage?: string;
  npmValid?: boolean;
  npmError?: string;
  pipPackage?: string;
  pipValid?: boolean;
  pipError?: string;
}

interface NpmPackageInfo {
  name: string;
  version: string;
  description?: string;
}

interface PyPIPackageInfo {
  info: {
    name: string;
    version: string;
    summary?: string;
  };
}

const ONE_YEAR_MS = 365 * 24 * 60 * 60 * 1000;

function parseGitHubUrl(url: string): { owner: string; repo: string } | null {
  const patterns = [
    /^https?:\/\/github\.com\/([^/]+)\/([^/]+?)(?:\/.*)?$/,
    /^github\.com\/([^/]+)\/([^/]+?)(?:\/.*)?$/,
  ];

  for (const pattern of patterns) {
    const match = url.match(pattern);
    if (match) {
      return {
        owner: match[1],
        repo: match[2].replace(/\.git$/, ''),
      };
    }
  }

  return null;
}

async function checkNpmPackage(
  packageName: string
): Promise<{ exists: boolean; error?: string; data?: NpmPackageInfo }> {
  const url = `https://registry.npmjs.org/${encodeURIComponent(packageName)}/latest`;

  try {
    const response = await fetch(url, {
      headers: {
        Accept: 'application/json',
        'User-Agent': 'octocode-mcp-validator',
      },
    });

    if (response.ok) {
      const data = (await response.json()) as NpmPackageInfo;
      return { exists: true, data };
    }

    if (response.status === 404) {
      return { exists: false, error: 'Package not found on npm' };
    }

    return { exists: false, error: `HTTP ${response.status}` };
  } catch (err) {
    return {
      exists: false,
      error: err instanceof Error ? err.message : 'Unknown error',
    };
  }
}

async function checkPipPackage(
  packageName: string
): Promise<{ exists: boolean; error?: string; data?: PyPIPackageInfo }> {
  const url = `https://pypi.org/pypi/${encodeURIComponent(packageName)}/json`;

  try {
    const response = await fetch(url, {
      headers: {
        Accept: 'application/json',
        'User-Agent': 'octocode-mcp-validator',
      },
    });

    if (response.ok) {
      const data = (await response.json()) as PyPIPackageInfo;
      return { exists: true, data };
    }

    if (response.status === 404) {
      return { exists: false, error: 'Package not found on PyPI' };
    }

    return { exists: false, error: `HTTP ${response.status}` };
  } catch (err) {
    return {
      exists: false,
      error: err instanceof Error ? err.message : 'Unknown error',
    };
  }
}

async function validateMCP(
  mcp: MCPRegistryEntry,
  checkPackages: boolean,
  token?: string | null
): Promise<ValidationResult> {
  const parsed = parseGitHubUrl(mcp.repository);

  if (!parsed) {
    return {
      id: mcp.id,
      name: mcp.name,
      repository: mcp.repository,
      status: 'error',
      error: 'Could not parse GitHub URL',
    };
  }

  const result = await checkGitHubRepository(
    parsed.owner,
    parsed.repo,
    'octocode-mcp-validator',
    token
  );

  if (!result.exists) {
    return {
      id: mcp.id,
      name: mcp.name,
      repository: mcp.repository,
      status: 'invalid',
      error: result.error,
      statusCode: result.statusCode,
    };
  }

  if (result.data?.archived) {
    return {
      id: mcp.id,
      name: mcp.name,
      repository: mcp.repository,
      status: 'invalid',
      error: 'Repository is archived',
      statusCode: result.statusCode,
      stars: result.data.stargazers_count,
    };
  }

  if (result.data?.disabled) {
    return {
      id: mcp.id,
      name: mcp.name,
      repository: mcp.repository,
      status: 'invalid',
      error: 'Repository is disabled',
      statusCode: result.statusCode,
      stars: result.data.stargazers_count,
    };
  }

  const lastPushed = result.data?.pushed_at
    ? new Date(result.data.pushed_at)
    : null;
  const isStale = lastPushed && Date.now() - lastPushed.getTime() > ONE_YEAR_MS;

  const validationResult: ValidationResult = {
    id: mcp.id,
    name: mcp.name,
    repository: mcp.repository,
    status: isStale ? 'warning' : 'valid',
    error: isStale
      ? 'Repository has not been updated in over 1 year'
      : undefined,
    statusCode: result.statusCode,
    stars: result.data?.stargazers_count,
    lastPushed: result.data?.pushed_at,
  };

  if (checkPackages && mcp.npmPackage && mcp.installationType === 'npx') {
    const npmResult = await checkNpmPackage(mcp.npmPackage);
    validationResult.npmPackage = mcp.npmPackage;
    validationResult.npmValid = npmResult.exists;
    if (!npmResult.exists) {
      validationResult.npmError = npmResult.error;
      if (validationResult.status === 'valid') {
        validationResult.status = 'warning';
        validationResult.error = `npm package not found: ${mcp.npmPackage}`;
      }
    }
  }

  if (checkPackages && mcp.pipPackage && mcp.installationType === 'pip') {
    const pipResult = await checkPipPackage(mcp.pipPackage);
    validationResult.pipPackage = mcp.pipPackage;
    validationResult.pipValid = pipResult.exists;
    if (!pipResult.exists) {
      validationResult.pipError = pipResult.error;
      if (validationResult.status === 'valid') {
        validationResult.status = 'warning';
        validationResult.error = `pip package not found: ${mcp.pipPackage}`;
      }
    }
  }

  return validationResult;
}

async function validateAllMCPs(
  concurrency: number = 5,
  delayMs: number = 100,
  checkPackages: boolean = false,
  token?: string | null
): Promise<ValidationResult[]> {
  const results: ValidationResult[] = [];
  const total = MCP_REGISTRY.length;

  console.log(`\n🔍 Validating ${total} MCP entries...`);
  if (checkPackages) {
    console.log('   (including npm/pip package validation)\n');
  } else {
    console.log(
      '   (use --check-packages to also validate npm/pip packages)\n'
    );
  }

  for (let i = 0; i < total; i += concurrency) {
    const batch = MCP_REGISTRY.slice(i, i + concurrency);
    const batchResults = await Promise.all(
      batch.map(mcp => validateMCP(mcp, checkPackages, token))
    );
    results.push(...batchResults);

    const progress = Math.min(i + concurrency, total);
    writeValidationProgress(results, progress, total);

    if (i + concurrency < total) {
      await new Promise(resolve => setTimeout(resolve, delayMs));
    }
  }

  console.log('\n');
  return results;
}

function printReport(results: ValidationResult[]): void {
  const { valid, warnings, invalid, errors, staleWarnings, otherWarnings } =
    splitValidationResults(results);

  printReportHeader('MCP REGISTRY VALIDATION REPORT');
  printSummary([
    ['Total MCPs:', results.length],
    ['✅ Valid:', valid.length],
    ['⚠️  Warnings:', warnings.length],
    ['🗑️  Stale:', staleWarnings.length],
    ['❌ Invalid:', invalid.length],
    ['🔴 Errors:', errors.length],
  ]);

  if (invalid.length > 0) {
    printSectionHeader(
      '❌ INVALID MCPs (Repository not found or inaccessible)'
    );
    for (const mcp of invalid) {
      console.log(`  • ${mcp.id}`);
      console.log(`    Name:       ${mcp.name}`);
      console.log(`    Repository: ${mcp.repository}`);
      console.log(`    Error:      ${mcp.error}`);
      if (mcp.statusCode) {
        console.log(`    Status:     HTTP ${mcp.statusCode}`);
      }
      console.log();
    }
  }

  if (staleWarnings.length > 0) {
    printSectionHeader('🗑️  STALE MCPs - CONSIDER REMOVING FROM REGISTRY');
    console.log(
      '   The following MCPs have not been updated in over 1 year and may be abandoned.'
    );
    console.log('   Consider removing them from mcp-registry.ts:\n');
    for (const mcp of staleWarnings) {
      console.log(`  • ${mcp.id}`);
      console.log(`    Name:       ${mcp.name}`);
      console.log(`    Repository: ${mcp.repository}`);
      if (mcp.lastPushed) {
        console.log(`    Last push:  ${formatRelativeTime(mcp.lastPushed)}`);
      }
      if (mcp.stars !== undefined) {
        console.log(`    Stars:      ${mcp.stars}`);
      }
      console.log();
    }
    console.log(
      '   👆 ACTION REQUIRED: Remove stale MCPs from mcp-registry.ts!\n'
    );
  }

  if (otherWarnings.length > 0) {
    printSectionHeader('⚠️  WARNINGS (Package issues)');
    for (const mcp of otherWarnings) {
      console.log(`  • ${mcp.id}`);
      console.log(`    Name:       ${mcp.name}`);
      console.log(`    Repository: ${mcp.repository}`);
      console.log(`    Warning:    ${mcp.error}`);
      if (mcp.lastPushed) {
        console.log(`    Last push:  ${formatRelativeTime(mcp.lastPushed)}`);
      }
      if (mcp.stars !== undefined) {
        console.log(`    Stars:      ${mcp.stars}`);
      }
      if (mcp.npmError) {
        console.log(`    npm:        ${mcp.npmError}`);
      }
      if (mcp.pipError) {
        console.log(`    pip:        ${mcp.pipError}`);
      }
      console.log();
    }
  }

  if (errors.length > 0) {
    printSectionHeader('🔴 ERRORS (Could not validate)');
    for (const mcp of errors) {
      console.log(`  • ${mcp.id}`);
      console.log(`    Name:       ${mcp.name}`);
      console.log(`    Repository: ${mcp.repository}`);
      console.log(`    Error:      ${mcp.error}`);
      console.log();
    }
  }

  const sortedByStars = topByStars(results, 10);

  if (sortedByStars.length > 0) {
    printSectionHeader('⭐ TOP 10 BY STARS', 40);
    for (const mcp of sortedByStars) {
      const stars = (mcp.stars ?? 0).toString().padStart(6);
      console.log(`  ${stars} ⭐  ${mcp.name}`);
    }
    console.log();
  }

  if (invalid.length === 0 && errors.length === 0) {
    console.log('✅ All MCP repositories are valid!\n');
  }

  console.log('═'.repeat(80));
}

function outputJson(results: ValidationResult[]): void {
  console.log(JSON.stringify(buildValidationJsonSummary(results), null, 2));
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const jsonOutput = args.includes('--json');
  const checkPackages = args.includes('--check-packages');
  const concurrency = parseInt(
    args.find(a => a.startsWith('--concurrency='))?.split('=')[1] || '5'
  );

  const token = await resolveValidatorToken();

  if (!jsonOutput) {
    printValidatorBanner('MCP REGISTRY VALIDATOR - octocode-cli');

    if (!token) {
      printRateLimitTip();
    }
  }

  const results = await validateAllMCPs(concurrency, 100, checkPackages, token);

  if (jsonOutput) {
    outputJson(results);
  } else {
    printReport(results);
  }

  process.exit(hasBlockingValidationFailures(results) ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
