import { spawn, execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { CONFIG_FIELDS, contractDriftMessage } from '@octocodeai/config';

type ManagementRequest = Record<string, unknown>;
type ManagementData = Record<string, unknown>;
type ManagementFailure = Error & { code: string; safe: boolean };

function failure(message: string, code = 'INVALID_INPUT'): ManagementFailure {
  return Object.assign(new Error(message), { code, safe: true });
}

/** Structured stdin is the only path from the web session to native config policy. */
export async function requestNativeConfig(
  bin: string,
  request: ManagementRequest,
  options: { signal?: AbortSignal; env?: NodeJS.ProcessEnv; cwd?: string } = {}
): Promise<ManagementData> {
  const { getPublicToolCatalogWithAddons } =
    await import('@octocodeai/config/schema');
  const expectedFingerprint = getPublicToolCatalogWithAddons({
    availableTools: [],
  }).fingerprint;
  const body = JSON.stringify({ ...request, expectedFingerprint });
  if (Buffer.byteLength(body) > 131_072)
    throw failure('Request exceeds 128 KiB.');
  if (options.signal?.aborted)
    throw failure('The config session is closed.', 'CLOSED');
  return new Promise((resolve, reject) => {
    const launcher = /\.[cm]?js$/.test(bin);
    const child = spawn(
      launcher ? process.execPath : bin,
      launcher ? [bin, 'config', '--manage'] : ['config', '--manage'],
      {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: options.env ?? process.env,
        cwd: options.cwd ?? process.cwd(),
        shell: false,
      }
    );
    let output = '';
    let bytes = 0;
    let done = false;
    const stop = (): void => {
      child.kill('SIGKILL');
    };
    const timeout = setTimeout(stop, 30_000);
    timeout.unref();
    options.signal?.addEventListener('abort', stop, { once: true });
    const finish = (error?: Error, data?: ManagementData): void => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      options.signal?.removeEventListener('abort', stop);
      if (error) reject(error);
      else resolve(data ?? {});
    };
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      bytes += Buffer.byteLength(chunk);
      if (bytes > 32 * 1024 * 1024) {
        stop();
        finish(
          failure(
            'Native configuration output exceeds the 32 MiB transport limit.',
            'OUTPUT_LIMIT'
          )
        );
      } else output += chunk;
    });
    // Native diagnostics stay outside the browser response and are never echoed.
    child.stderr.resume();
    child.stdin.on('error', () => {
      /* The close event owns the result. */
    });
    child.once('error', () =>
      finish(
        failure('Cannot start native configuration management.', 'UNAVAILABLE')
      )
    );
    child.once('close', code => {
      if (done) return;
      let response: Record<string, unknown>;
      try {
        response = JSON.parse(output) as Record<string, unknown>;
      } catch {
        finish(
          failure(
            'Native configuration management is unavailable or incompatible. Rebuild or update Octocode.',
            'UNAVAILABLE'
          )
        );
        return;
      }
      if (response.apiVersion !== 1) {
        finish(
          failure(
            'Configuration management versions do not match. Update Octocode.',
            'UNAVAILABLE'
          )
        );
        return;
      }
      if (code !== 0 || response.success !== true) {
        const nativeError = response.error as
          { code?: unknown; message?: unknown } | undefined;
        const allowed = ['CONFLICT', 'INVALID_INPUT', 'FORBIDDEN'];
        const nativeCode =
          typeof nativeError?.code === 'string' &&
          allowed.includes(nativeError.code)
            ? nativeError.code
            : 'INVALID_INPUT';
        finish(
          failure(
            typeof nativeError?.message === 'string'
              ? nativeError.message
              : 'Configuration change failed.',
            nativeCode
          )
        );
        return;
      }
      if (response.fingerprint !== expectedFingerprint) {
        finish(
          failure(
            contractDriftMessage(
              expectedFingerprint,
              String(response.fingerprint)
            ),
            'UNAVAILABLE'
          )
        );
        return;
      }
      if (
        !response.data ||
        typeof response.data !== 'object' ||
        Array.isArray(response.data)
      ) {
        finish(
          failure('Native configuration response is invalid.', 'UNAVAILABLE')
        );
        return;
      }
      const data = response.data as ManagementData;
      if (request.operation === 'inspect' && Array.isArray(data.settings)) {
        data.settings = data.settings.map((item: Record<string, unknown>) => ({
          ...CONFIG_FIELDS.find(field => field.path === item.key),
          ...item,
        }));
      }
      finish(undefined, data);
    });
    child.stdin.end(body);
  });
}

async function openBrowser(url: string): Promise<void> {
  const exec = promisify(execFile);
  if (process.platform === 'darwin')
    await exec('/usr/bin/open', [url], { timeout: 10_000 });
  else if (process.platform === 'win32')
    await exec('rundll32.exe', ['url.dll,FileProtocolHandler', url], {
      timeout: 10_000,
    });
  else await exec('xdg-open', [url], { timeout: 10_000 });
}

export async function configViewCommand(
  bin: string,
  argv: readonly string[]
): Promise<number> {
  let noOpen = false;
  let idleTimeout = 900;
  const commandIndex = argv.indexOf('config');
  const flags = [
    ...argv.slice(0, commandIndex),
    ...argv.slice(commandIndex + 2),
  ];
  for (let index = 0; index < flags.length; index++) {
    const flag = flags[index];
    if (flag === '--no-open') noOpen = true;
    else if (flag === '--idle-timeout' || flag?.startsWith('--idle-timeout=')) {
      const raw =
        flag === '--idle-timeout' ? flags[++index] : flag.split('=')[1];
      if (!raw || !/^\d+$/.test(raw)) {
        console.error(
          '--idle-timeout must be an integer from 30 to 3600 seconds.'
        );
        return 2;
      }
      idleTimeout = Number(raw);
      if (idleTimeout < 30 || idleTimeout > 3600) {
        console.error(
          '--idle-timeout must be an integer from 30 to 3600 seconds.'
        );
        return 2;
      }
    } else if (flag === '--no-color' || flag === '--json-errors') {
      /* Global presentation flags. */
    } else {
      console.error(
        'Usage: octocode config view [--no-open] [--idle-timeout 30..3600]'
      );
      return 2;
    }
  }
  const abort = new AbortController();
  const signals: NodeJS.Signals[] = ['SIGINT', 'SIGTERM', 'SIGHUP'];
  const saved = new Map(
    signals.map(signal => [signal, process.listeners(signal)])
  );
  const shutdown = (): void => abort.abort();
  for (const signal of signals) {
    process.removeAllListeners(signal);
    process.on(signal, shutdown);
  }
  try {
    // Probe the API and fail closed before exposing a browser session.
    await requestNativeConfig(
      bin,
      { operation: 'inspect' },
      { signal: abort.signal }
    );
    const { startConfigView } = await import('../config-view/server.js');
    const session = await startConfigView({
      request: (request: ManagementRequest, context: { signal: AbortSignal }) =>
        requestNativeConfig(bin, request, { signal: context.signal }),
      signal: abort.signal,
      idleTimeoutMs: idleTimeout * 1000,
      open: noOpen ? undefined : openBrowser,
      onReady: (session: { url: string }) => {
        console.log(
          `Octocode config view: ${session.url}\nPress Ctrl+C to close. This temporary URL grants access to your configuration.`
        );
      },
    });
    await session.closed;
    return 0;
  } catch (error) {
    if (abort.signal.aborted) return 0;
    console.error(
      error instanceof Error && 'safe' in error && error.safe === true
        ? error.message
        : 'Cannot open the config view. Check the Octocode installation.'
    );
    return 5;
  } finally {
    abort.abort();
    for (const signal of signals) {
      process.removeListener(signal, shutdown);
      for (const listener of saved.get(signal) ?? [])
        process.on(signal, listener);
    }
  }
}
