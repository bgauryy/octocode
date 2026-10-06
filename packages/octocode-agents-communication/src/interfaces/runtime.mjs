import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
export const script = fileURLToPath(new URL('../scripts/communication.py', import.meta.url));
export function python() { return process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3'); }
export function execute(args, { stream = false, input } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(python(), ['-B', script, ...args], { stdio: [input === undefined ? 'inherit' : 'pipe', stream ? 'inherit' : 'pipe', 'inherit'] });
    let inputError;
    if (input !== undefined) { child.stdin.on('error', error => { inputError = error; }); child.stdin.end(input); }
    const handlers = new Map(['SIGINT', 'SIGTERM'].map(signal => [signal, () => child.kill(signal)]));
    for (const [signal, handler] of handlers) process.on(signal, handler);
    let output = '';
    if (!stream) child.stdout.setEncoding('utf8').on('data', chunk => { output += chunk; });
    const cleanup = () => { for (const [signal, handler] of handlers) process.off(signal, handler); };
    child.on('error', error => { cleanup(); reject(new Error(`Cannot start Python; set OCTOCODE_PYTHON to an installed interpreter: ${error.message}`)); });
    child.on('close', (code, signal) => {
      cleanup();
      if (signal) return reject(Object.assign(new Error(`Communication runtime terminated by ${signal}`), { exitCode: signal === 'SIGINT' ? 130 : signal === 'SIGTERM' ? 143 : 1 }));
      if (code !== 0) return reject(Object.assign(new Error(`Communication runtime exited ${code}`), { exitCode: code }));
      if (inputError) return reject(new Error(`Cannot deliver command input: ${inputError.message}`));
      if (stream) return resolve(undefined);
      try { resolve(JSON.parse(output)); } catch { reject(new Error('Communication runtime returned invalid JSON')); }
    });
  });
}
