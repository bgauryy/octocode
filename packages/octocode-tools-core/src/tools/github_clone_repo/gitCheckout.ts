import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';
import { TOOLING_ALLOWED_ENV_VARS } from '../../utils/exec/spawn/env.js';

const CLONE_TIMEOUT_MS = 2 * 60 * 1000;
const SPARSE_CHECKOUT_TIMEOUT_MS = 30 * 1000;
const GIT_ALLOWED_ENV_VARS = [
  ...TOOLING_ALLOWED_ENV_VARS,
  'GIT_TERMINAL_PROMPT',
] as const;

export async function readHeadCommit(directory: string): Promise<string> {
  const sha = (
    await runGit(
      ['-C', directory, 'rev-parse', '--verify', 'HEAD^{commit}'],
      5_000,
      'read checkout HEAD'
    )
  )
    .trim()
    .toLowerCase();
  if (!/^[a-f0-9]{40}$/.test(sha))
    throw new Error('Git returned an invalid checkout HEAD commit SHA.');
  return sha;
}

export async function executeCommitClone(
  commit: string,
  targetDir: string,
  sparsePath: string | undefined,
  url: string,
  token?: string
): Promise<void> {
  await runGit(
    ['init', '--', targetDir],
    CLONE_TIMEOUT_MS,
    'initialize commit checkout',
    token
  );
  const scoped = [...buildAuthArgs(url, token), '-C', targetDir];
  await runGit(
    [...scoped, 'remote', 'add', 'origin', url],
    CLONE_TIMEOUT_MS,
    'configure commit remote',
    token
  );
  if (sparsePath) {
    await runGit(
      [...scoped, 'config', 'remote.origin.promisor', 'true'],
      SPARSE_CHECKOUT_TIMEOUT_MS,
      'configure sparse promisor',
      token
    );
    await runGit(
      [...scoped, 'config', 'remote.origin.partialclonefilter', 'blob:none'],
      SPARSE_CHECKOUT_TIMEOUT_MS,
      'configure sparse filter',
      token
    );
  }
  await runGit(
    [
      ...scoped,
      'fetch',
      '--depth',
      '1',
      ...(sparsePath ? ['--filter', 'blob:none'] : []),
      '--',
      'origin',
      commit,
    ],
    CLONE_TIMEOUT_MS,
    'fetch requested commit',
    token
  );
  if (sparsePath) {
    await runGit(
      [
        ...scoped,
        'sparse-checkout',
        'set',
        '--cone',
        '--skip-checks',
        '--',
        sparsePath,
      ],
      SPARSE_CHECKOUT_TIMEOUT_MS,
      'set sparse commit paths',
      token
    );
  }
  await runGit(
    [...scoped, 'checkout', '--detach', 'FETCH_HEAD', '--'],
    CLONE_TIMEOUT_MS,
    'check out requested commit',
    token
  );
}

export async function executeFullClone(
  owner: string,
  repo: string,
  branch: string,
  targetDir: string,
  url: string,
  token?: string
): Promise<void> {
  const args = buildAuthArgs(url, token);
  args.push(
    'clone',
    '--depth',
    '1',
    '--single-branch',
    '--branch',
    branch,
    '--',
    url,
    targetDir
  );
  await runGit(args, CLONE_TIMEOUT_MS, `full clone of ${owner}/${repo}`, token);
}

export async function executeSparseClone(
  owner: string,
  repo: string,
  branch: string,
  targetDir: string,
  sparsePath: string,
  url: string,
  token?: string
): Promise<void> {
  const cloneArgs = buildAuthArgs(url, token);
  cloneArgs.push(
    'clone',
    '--filter',
    'blob:none',
    '--sparse',
    '--depth',
    '1',
    '--single-branch',
    '--branch',
    branch,
    '--',
    url,
    targetDir
  );
  await runGit(
    cloneArgs,
    CLONE_TIMEOUT_MS,
    `sparse clone of ${owner}/${repo}`,
    token
  );

  const sparseArgs: string[] = [
    ...buildAuthArgs(url, token),
    '-C',
    targetDir,
    'sparse-checkout',
    'set',
    '--skip-checks',
    '--',
    sparsePath,
  ];
  await runGit(
    sparseArgs,
    SPARSE_CHECKOUT_TIMEOUT_MS,
    `sparse-checkout set ${sparsePath}`,
    token
  );
}

function buildAuthArgs(url: string, token?: string): string[] {
  if (!token) return [];
  return ['-c', `http.${url}.extraHeader=Authorization: Bearer ${token}`];
}

export async function assertGitAvailable(): Promise<void> {
  try {
    const result = await spawnWithTimeout('git', ['--version'], {
      timeout: 5_000,
      maxOutputSize: 1024,
      allowEnvVars: GIT_ALLOWED_ENV_VARS,
      env: { GIT_TERMINAL_PROMPT: '0' },
    });
    if (!result.success) {
      throw new Error('git --version returned non-zero');
    }
  } catch {
    throw new Error(
      'git is not installed or not on PATH. ' +
        'The ghCloneRepo tool requires git to be available.'
    );
  }
}

function scrubToken(text: string, token?: string): string {
  let scrubbed = text;
  if (token) {
    scrubbed = scrubbed.replaceAll(token, '[REDACTED]');
  }
  scrubbed = scrubbed.replace(
    /Authorization:\s*Bearer\s+\S+/gi,
    'Authorization: Bearer [REDACTED]'
  );
  scrubbed = scrubbed.replace(
    /Authorization:\s*token\s+\S+/gi,
    'Authorization: token [REDACTED]'
  );
  return scrubbed;
}

async function runGit(
  args: string[],
  timeout: number,
  label: string,
  token?: string
): Promise<string> {
  const result = await spawnWithTimeout('git', args, {
    timeout,
    maxOutputSize: 5 * 1024 * 1024,
    allowEnvVars: GIT_ALLOWED_ENV_VARS,
    env: { GIT_TERMINAL_PROMPT: '0' },
  });

  if (!result.success) {
    const stderr = scrubToken(result.stderr?.trim() || '', token);
    const suffix = stderr ? `: ${stderr}` : '';
    throw new Error(`git ${label} failed${suffix}`);
  }
  return result.stdout ?? '';
}
