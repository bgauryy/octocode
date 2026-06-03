#!/usr/bin/env npx tsx

import {
  SKILLS_MARKETPLACES,
  type MarketplaceSource,
} from '../src/configs/skills-marketplace.js';
import {
  buildGitHubApiHeaders,
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
  owner: string;
  repo: string;
  url: string;
  skillsPathValid?: boolean;
  skillsPathError?: string;
  skillsCount?: number;
}

interface GitHubContentItem {
  name: string;
  path: string;
  type: 'file' | 'dir';
}

const ONE_YEAR_MS = 365 * 24 * 60 * 60 * 1000;

async function checkSkillsPath(
  owner: string,
  repo: string,
  path: string,
  branch: string,
  skillPattern: 'flat-md' | 'skill-folders',
  token?: string | null
): Promise<{
  exists: boolean;
  error?: string;
  skillsCount?: number;
}> {

  const apiPath = path || '';
  const url = `https://api.github.com/repos/${owner}/${repo}/contents/${apiPath}?ref=${branch}`;

  try {
    const response = await fetch(url, {
      headers: buildGitHubApiHeaders('octocode-skills-validator', token),
    });

    if (!response.ok) {
      if (response.status === 404) {
        return { exists: false, error: `Skills path '${path}' not found` };
      }
      return { exists: false, error: `HTTP ${response.status}` };
    }

    const contents = (await response.json()) as GitHubContentItem[];

    if (!Array.isArray(contents)) {
      return { exists: false, error: 'Skills path is not a directory' };
    }

    let skillsCount = 0;
    if (skillPattern === 'flat-md') {

      skillsCount = contents.filter(
        item =>
          item.type === 'file' &&
          item.name.endsWith('.md') &&
          item.name.toLowerCase() !== 'readme.md'
      ).length;
    } else {

      skillsCount = contents.filter(
        item =>
          item.type === 'dir' &&
          !item.name.startsWith('.') &&
          item.name.toLowerCase() !== 'node_modules'
      ).length;
    }

    return { exists: true, skillsCount };
  } catch (err) {
    return {
      exists: false,
      error: err instanceof Error ? err.message : 'Unknown error',
    };
  }
}

async function validateMarketplace(
  marketplace: MarketplaceSource,
  checkSkills: boolean,
  token?: string | null
): Promise<ValidationResult> {
  const result = await checkGitHubRepository(
    marketplace.owner,
    marketplace.repo,
    'octocode-skills-validator',
    token
  );

  if (!result.exists) {
    return {
      id: marketplace.id,
      name: marketplace.name,
      owner: marketplace.owner,
      repo: marketplace.repo,
      url: marketplace.url,
      status: 'invalid',
      error: result.error,
      statusCode: result.statusCode,
    };
  }

  if (result.data?.archived) {
    return {
      id: marketplace.id,
      name: marketplace.name,
      owner: marketplace.owner,
      repo: marketplace.repo,
      url: marketplace.url,
      status: 'invalid',
      error: 'Repository is archived',
      statusCode: result.statusCode,
      stars: result.data.stargazers_count,
    };
  }

  if (result.data?.disabled) {
    return {
      id: marketplace.id,
      name: marketplace.name,
      owner: marketplace.owner,
      repo: marketplace.repo,
      url: marketplace.url,
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
    id: marketplace.id,
    name: marketplace.name,
    owner: marketplace.owner,
    repo: marketplace.repo,
    url: marketplace.url,
    status: isStale ? 'warning' : 'valid',
    error: isStale
      ? 'Repository has not been updated in over 1 year'
      : undefined,
    statusCode: result.statusCode,
    stars: result.data?.stargazers_count,
    lastPushed: result.data?.pushed_at,
  };

  if (checkSkills) {
    const skillsResult = await checkSkillsPath(
      marketplace.owner,
      marketplace.repo,
      marketplace.skillsPath,
      marketplace.branch,
      marketplace.skillPattern,
      token
    );

    validationResult.skillsPathValid = skillsResult.exists;
    validationResult.skillsCount = skillsResult.skillsCount;

    if (!skillsResult.exists) {
      validationResult.skillsPathError = skillsResult.error;
      if (validationResult.status === 'valid') {
        validationResult.status = 'warning';
        validationResult.error = skillsResult.error;
      }
    } else if (skillsResult.skillsCount === 0) {
      validationResult.skillsPathError = 'No skills found in path';
      if (validationResult.status === 'valid') {
        validationResult.status = 'warning';
        validationResult.error = 'No skills found in skills path';
      }
    }
  }

  return validationResult;
}

async function validateAllMarketplaces(
  concurrency: number = 3,
  delayMs: number = 200,
  checkSkills: boolean = false,
  token?: string | null
): Promise<ValidationResult[]> {
  const results: ValidationResult[] = [];
  const total = SKILLS_MARKETPLACES.length;

  console.log(`\n🔍 Validating ${total} skills marketplace entries...`);
  if (checkSkills) {
    console.log('   (including skills path validation)\n');
  } else {
    console.log(
      '   (use --check-skills to also validate skills directories)\n'
    );
  }

  for (let i = 0; i < total; i += concurrency) {
    const batch = SKILLS_MARKETPLACES.slice(i, i + concurrency);
    const batchResults = await Promise.all(
      batch.map(m => validateMarketplace(m, checkSkills, token))
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

  const totalSkills = results.reduce((sum, r) => sum + (r.skillsCount || 0), 0);
  const summaryRows: Array<[string, string | number]> = [
    ['Total Marketplaces:', results.length],
    ['✅ Valid:', valid.length],
    ['⚠️  Warnings:', warnings.length],
    ['🗑️  Stale:', staleWarnings.length],
    ['❌ Invalid:', invalid.length],
    ['🔴 Errors:', errors.length],
  ];

  if (totalSkills > 0) {
    summaryRows.push(['📚 Total Skills:', totalSkills]);
  }

  printReportHeader('SKILLS MARKETPLACE VALIDATION REPORT');
  printSummary(summaryRows);

  if (invalid.length > 0) {
    printSectionHeader(
      '❌ INVALID MARKETPLACES (Repository not found or inaccessible)'
    );
    for (const m of invalid) {
      console.log(`  • ${m.id}`);
      console.log(`    Name:       ${m.name}`);
      console.log(`    Repository: ${m.owner}/${m.repo}`);
      console.log(`    URL:        ${m.url}`);
      console.log(`    Error:      ${m.error}`);
      if (m.statusCode) {
        console.log(`    Status:     HTTP ${m.statusCode}`);
      }
      console.log();
    }
  }

  if (staleWarnings.length > 0) {
    printSectionHeader(
      '🗑️  STALE MARKETPLACES - CONSIDER REMOVING FROM REGISTRY'
    );
    console.log(
      '   The following marketplaces have not been updated in over 1 year and may be abandoned.'
    );
    console.log('   Consider removing them from skills-marketplace.ts:\n');
    for (const m of staleWarnings) {
      console.log(`  • ${m.id}`);
      console.log(`    Name:       ${m.name}`);
      console.log(`    Repository: ${m.owner}/${m.repo}`);
      if (m.lastPushed) {
        console.log(`    Last push:  ${formatRelativeTime(m.lastPushed)}`);
      }
      if (m.stars !== undefined) {
        console.log(`    Stars:      ${m.stars}`);
      }
      console.log();
    }
    console.log(
      '   👆 ACTION REQUIRED: Remove stale marketplaces from skills-marketplace.ts!\n'
    );
  }

  if (otherWarnings.length > 0) {
    printSectionHeader('⚠️  WARNINGS (Skills path issues)');
    for (const m of otherWarnings) {
      console.log(`  • ${m.id}`);
      console.log(`    Name:       ${m.name}`);
      console.log(`    Repository: ${m.owner}/${m.repo}`);
      console.log(`    Warning:    ${m.error}`);
      if (m.lastPushed) {
        console.log(`    Last push:  ${formatRelativeTime(m.lastPushed)}`);
      }
      if (m.stars !== undefined) {
        console.log(`    Stars:      ${m.stars}`);
      }
      if (m.skillsPathError) {
        console.log(`    Skills:     ${m.skillsPathError}`);
      }
      console.log();
    }
  }

  if (errors.length > 0) {
    printSectionHeader('🔴 ERRORS (Could not validate)');
    for (const m of errors) {
      console.log(`  • ${m.id}`);
      console.log(`    Name:       ${m.name}`);
      console.log(`    Repository: ${m.owner}/${m.repo}`);
      console.log(`    Error:      ${m.error}`);
      console.log();
    }
  }

  const sortedByStars = topByStars(results);

  if (sortedByStars.length > 0) {
    printSectionHeader('⭐ MARKETPLACES BY STARS', 40);
    for (const m of sortedByStars) {
      const stars = (m.stars ?? 0).toString().padStart(6);
      const skills =
        m.skillsCount !== undefined ? ` (${m.skillsCount} skills)` : '';
      console.log(`  ${stars} ⭐  ${m.name}${skills}`);
    }
    console.log();
  }

  if (invalid.length === 0 && errors.length === 0) {
    console.log('✅ All skills marketplace repositories are valid!\n');
  }

  console.log('═'.repeat(80));
}

function outputJson(results: ValidationResult[]): void {
  console.log(
    JSON.stringify(
      buildValidationJsonSummary(results, {
        totalSkills: results.reduce((sum, r) => sum + (r.skillsCount || 0), 0),
      }),
      null,
      2
    )
  );
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const jsonOutput = args.includes('--json');
  const checkSkills = args.includes('--check-skills');
  const concurrency = parseInt(
    args.find(a => a.startsWith('--concurrency='))?.split('=')[1] || '3'
  );

  const token = await resolveValidatorToken();

  if (!jsonOutput) {
    printValidatorBanner('SKILLS MARKETPLACE VALIDATOR - octocode-cli');

    if (!token) {
      printRateLimitTip();
    }
  }

  const results = await validateAllMarketplaces(concurrency, 200, checkSkills, token);

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
