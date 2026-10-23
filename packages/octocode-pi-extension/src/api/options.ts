import { envFlag } from '../shared/env.js';

const API_ENV = 'OCTOCODE_API';
const API_HTTP_ENV = 'OCTOCODE_API_HTTP';

export interface ApiOptions {
  /** Serve on a Unix socket (default when enabled; not available on Windows). */
  socket: boolean;
  /** Serve on loopback HTTP: a port, 0 for any free port, or undefined for no HTTP. */
  http?: number | undefined;
}

/** The transports the environment asks for, or undefined when the API is off. A bad port throws its reason instead of silently serving nothing. */
export function optionsFromEnv(env: NodeJS.ProcessEnv = process.env, platform = process.platform): ApiOptions | undefined {
  const httpText = env[API_HTTP_ENV]?.trim();
  const port = httpText === undefined || httpText === '' ? undefined : /^\d+$/.test(httpText) ? Number(httpText) : Number.NaN;
  if (port !== undefined && !(port >= 0 && port <= 65_535)) throw new Error(`Octocode API is off: ${API_HTTP_ENV}=${httpText} is not a port (0-65535; 0 picks a free one).`);
  const on = envFlag(env, API_ENV);
  if (!on) return port === undefined ? undefined : { socket: false, http: port };
  return transportsFor(port, platform);
}

/** The transports of an API that is switched on: a Unix socket where there are any, and loopback HTTP on `http` (0: a free port) or, where there is no socket, on a free port. */
export function transportsFor(http: number | undefined, platform = process.platform): ApiOptions {
  const socket = platform !== 'win32';
  return { socket, http: http ?? (socket ? undefined : 0) };
}
