import { validateCommand } from 'octocode-security-utils/commandValidator';

const getSpawn = async () => {
  const { spawn } = await import('child_process');
  return spawn;
};

export async function spawnCollectOutput(
  command: string,
  args: string[],
  options: { maxBuffer?: number; timeout?: number } = {}
): Promise<{ stdout: string }> {
  const validation = validateCommand(command, args);
  if (!validation.isValid) {
    throw new Error(
      `Command validation failed: ${validation.error || 'Command not allowed'}`
    );
  }

  const spawnFn = await getSpawn();
  const { maxBuffer = 10 * 1024 * 1024, timeout = 30000 } = options;

  return new Promise((resolve, reject) => {
    const child = spawnFn(command, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout,
      env: {
        ...Object.fromEntries(
          ['PATH', 'HOME', 'USER', 'LANG', 'TERM', 'SHELL'].map(k => [
            k,
            process.env[k],
          ])
        ),
      },
    });

    let stdout = '';
    let totalSize = 0;

    child.stdout?.on('data', (data: Buffer) => {
      totalSize += data.length;
      if (totalSize > maxBuffer) {
        child.kill('SIGKILL');
        reject(new Error('Output size limit exceeded'));
        return;
      }
      stdout += data.toString();
    });

    child.on('close', code => {
      if (code === 0 || code === 1) {
        resolve({ stdout });
      } else {
        reject(
          Object.assign(new Error(`Process exited with code ${code}`), { code })
        );
      }
    });

    child.on('error', reject);
  });
}
