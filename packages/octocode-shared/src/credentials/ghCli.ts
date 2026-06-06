import { execFile } from 'child_process';

const COMMON_GH_PATHS = [
  '/opt/homebrew/bin',
  '/usr/local/bin',
  '/home/linuxbrew/.linuxbrew/bin',
];

export function getGhCliToken(hostname?: string): Promise<string | null> {
  return new Promise(resolve => {
    const args = ['auth', 'token'];
    if (hostname) args.push('--hostname', hostname);

    const existingPath = process.env.PATH ?? '';
    const existingParts = new Set(existingPath.split(':'));
    const extra = COMMON_GH_PATHS.filter(p => !existingParts.has(p));
    const augmentedPath = extra.length
      ? `${extra.join(':')}:${existingPath}`
      : existingPath;

    execFile(
      'gh',
      args,
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, PATH: augmentedPath },
      },
      (error, stdout) => {
        if (error || !stdout) {
          resolve(null);
          return;
        }
        resolve(stdout.trim() || null);
      }
    );
  });
}
