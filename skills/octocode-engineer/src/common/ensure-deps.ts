import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const REQUIRED_PACKAGES = [
  'typescript',
  '@ast-grep/napi',
  '@ast-grep/lang-python',
  'tree-sitter',
  'tree-sitter-typescript',
  'tree-sitter-python',
];

const MAX_SKILL_DIR_HOPS = 6;

type PackageManager = 'pnpm' | 'yarn' | 'npm';

interface InstallPlan {
  pm: PackageManager;
  cmd: string;
  args: string[];
  humanCommand: string;
}

function detectPackageManager(skillDir: string): PackageManager {
  if (existsSync(join(skillDir, 'pnpm-lock.yaml'))) return 'pnpm';
  if (existsSync(join(skillDir, 'yarn.lock'))) return 'yarn';
  return 'npm';
}

function planInstall(skillDir: string): InstallPlan {
  const pm = detectPackageManager(skillDir);
  switch (pm) {
    case 'pnpm':
      return {
        pm,
        cmd: 'pnpm',
        args: ['install', '--prod=false'],
        humanCommand: 'pnpm install',
      };
    case 'yarn':
      return {
        pm,
        cmd: 'yarn',
        args: ['install'],
        humanCommand: 'yarn install',
      };
    case 'npm':
      return {
        pm,
        cmd: 'npm',
        args: [
          'install',
          '--prefix',
          skillDir,
          '--no-audit',
          '--no-fund',
          '--legacy-peer-deps',
        ],
        humanCommand: 'npm install --legacy-peer-deps',
      };
  }
}

function findSkillDir(entryUrl: string): string {
  let dir = dirname(fileURLToPath(entryUrl));
  for (let i = 0; i < MAX_SKILL_DIR_HOPS; i++) {
    if (existsSync(join(dir, 'package.json'))) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return dirname(dirname(fileURLToPath(entryUrl)));
}

function missingPackagesFrom(skillDir: string, entryUrl: string): string[] {
  const nodeModulesDir = join(skillDir, 'node_modules');
  const requireFromEntry = createRequire(entryUrl);
  return REQUIRED_PACKAGES.filter((pkg) => {
    if (existsSync(join(nodeModulesDir, pkg))) return false;
    try {
      requireFromEntry.resolve(pkg, { paths: [skillDir] });
      return false;
    } catch {
      return true;
    }
  });
}

export interface EnsureDepsOptions {
  
  autoInstall?: boolean;
  
  tag?: string;
}


export function ensureNativeDependencies(
  entryUrl: string,
  options: EnsureDepsOptions = {},
): void {
  const envOptOut = process.env.OCTOCODE_NO_AUTO_INSTALL === '1';
  const { autoInstall = !envOptOut, tag = '[octocode-engineer]' } = options;
  const skillDir = findSkillDir(entryUrl);

  const missing = missingPackagesFrom(skillDir, entryUrl);
  if (missing.length === 0) return;

  const plan = planInstall(skillDir);

  process.stderr.write(
    `${tag} Missing runtime dependencies: ${missing.join(', ')}\n` +
      `${tag} Skill directory: ${skillDir}\n` +
      `${tag} Detected package manager: ${plan.pm} (from lockfile or default)\n`,
  );

  if (!autoInstall) {
    process.stderr.write(
      `${tag} Auto-install disabled (OCTOCODE_NO_AUTO_INSTALL=1). To install, run:\n` +
        `    cd ${skillDir}\n` +
        `    ${plan.humanCommand}\n`,
    );
    process.exit(1);
  }

  process.stderr.write(`${tag} Installing now: ${plan.cmd} ${plan.args.join(' ')}\n`);
  const result = spawnSync(plan.cmd, plan.args, {
    cwd: skillDir,
    stdio: 'inherit',
    shell: false,
  });

  if (result.status !== 0) {
    const hint =
      result.error && (result.error as NodeJS.ErrnoException).code === 'ENOENT'
        ? ` (command "${plan.cmd}" not found on PATH)`
        : '';
    process.stderr.write(
      `${tag} Install failed${hint}.\n` +
        `${tag} Please run manually:\n` +
        `    cd ${skillDir}\n` +
        `    ${plan.humanCommand}\n`,
    );
    process.exit(1);
  }

  const stillMissing = missingPackagesFrom(skillDir, entryUrl);
  if (stillMissing.length > 0) {
    process.stderr.write(
      `${tag} Install completed but still missing: ${stillMissing.join(', ')}\n` +
        `${tag} Check ${skillDir}/node_modules and retry:\n` +
        `    cd ${skillDir}\n` +
        `    ${plan.humanCommand}\n`,
    );
    process.exit(1);
  }

  process.stderr.write(`${tag} Dependencies installed.\n`);
}
