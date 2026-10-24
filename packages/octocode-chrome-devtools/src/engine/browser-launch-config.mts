// How a fresh Chrome is launched: the proxy from flags or config files, and the command-line flags.
import { existsSync, readFileSync } from 'fs';
import { join } from 'path';
import { errorMessage } from './error-message.mjs';
import { getOctocodeHome } from './octocode-config.mjs';

type Fail = (message: string) => never;
type LaunchIo = { fail: Fail; warn: (line: string) => void };
type Fields = { [key: string]: unknown };
type ProxySettings = { server: string; bypassList: string; pacUrl: string };
type ProxyConfig = ProxySettings & { enabled: boolean };

/** Every non-null object, arrays included, reads as optional fields, like `value?.key` does. */
const isFields = (value: unknown): value is Fields =>
  typeof value === 'object' && value !== null;
const fieldsOf = (value: unknown) => (isFields(value) ? value : undefined);

function readJsonFile(
  filePath: string,
  strict: boolean,
  { fail, warn }: LaunchIo
): unknown {
  if (!filePath || !existsSync(filePath)) return null;
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch (jsonErr) {
    if (strict)
      fail(
        `Invalid JSON in config file: ${filePath} (${errorMessage(jsonErr)})`
      );
    warn(
      `[BROWSER] Warning: ignoring invalid JSON config ${filePath}: ${errorMessage(jsonErr)}`
    );
    return null;
  }
}

const trimmed = (value: unknown) =>
  typeof value === 'string' ? value.trim() : '';

function normalizeProxyConfig(raw: unknown): ProxyConfig | null {
  if (!isFields(raw)) return null;
  return {
    enabled: raw.enabled === undefined ? true : Boolean(raw.enabled),
    server: trimmed(raw.server),
    bypassList: trimmed(raw.bypassList),
    pacUrl: trimmed(raw.pacUrl),
  };
}

const proxyCandidates = (json: Fields) => [
  json.proxy,
  fieldsOf(json.chromeDevtools)?.proxy,
  fieldsOf(fieldsOf(json.skills)?.['octocode-chrome-devtools'])?.proxy,
];

/**
 * The first proxy block in --config, else <cwd>/.octocode/chrome-devtools.json, else the
 * Octocode home config.json. A missing or invalid --config file fails.
 */
export function loadProxyConfig(
  configPath: string,
  io: LaunchIo
): ProxyConfig | null {
  const cwdConfig = join(process.cwd(), '.octocode', 'chrome-devtools.json');
  const homeConfig = join(getOctocodeHome(), 'config.json');
  if (configPath && !existsSync(configPath))
    io.fail(`Config file not found: ${configPath}`);
  const candidateFiles = configPath ? [configPath] : [cwdConfig, homeConfig];
  for (const configFile of candidateFiles) {
    const json = readJsonFile(configFile, Boolean(configPath), io);
    if (!isFields(json)) continue;
    for (const candidate of proxyCandidates(json)) {
      const normalized = normalizeProxyConfig(candidate);
      if (normalized) return normalized;
    }
  }
  return null;
}

/** Proxy flags win over an enabled proxy config. */
export function effectiveProxy(
  flags: ProxySettings,
  config: ProxyConfig | null
): ProxySettings {
  const fromConfig = config?.enabled ? config : null;
  return {
    server: flags.server || (fromConfig ? fromConfig.server : ''),
    bypassList: flags.bypassList || (fromConfig ? fromConfig.bypassList : ''),
    pacUrl: flags.pacUrl || (fromConfig ? fromConfig.pacUrl : ''),
  };
}

function displayArgs(headless: boolean, os: string) {
  if (!headless) return [];
  return [
    '--headless=new',
    '--disable-dev-shm-usage',
    ...(os === 'linux'
      ? ['--disable-gpu', '--no-sandbox', '--disable-setuid-sandbox']
      : []),
  ];
}

function proxyArgs(proxy: ProxySettings) {
  return [
    ...(proxy.server ? [`--proxy-server=${proxy.server}`] : []),
    ...(proxy.bypassList ? [`--proxy-bypass-list=${proxy.bypassList}`] : []),
    ...(proxy.pacUrl && !proxy.server
      ? [`--proxy-pac-url=${proxy.pacUrl}`]
      : []),
  ];
}

/** What open-browser launches: the port, profile, display, options and proxy. */
type LaunchOptions = {
  port: string;
  userDataDir: string;
  isolated: boolean;
  profile: string;
  headless: boolean;
  os: string;
  userAgent: string;
  windowSize: string;
  enableFeatures: string;
  proxy: ProxySettings;
  url: string;
};

function optionArgs(launch: LaunchOptions) {
  return [
    ...(launch.userAgent ? [`--user-agent=${launch.userAgent}`] : []),
    ...(launch.windowSize
      ? [`--window-size=${launch.windowSize.replace('x', ',')}`]
      : []),
    ...(launch.enableFeatures
      ? [`--enable-features=${launch.enableFeatures}`]
      : []),
  ];
}

/** The Chrome command line for a fresh browser on launch.port. */
export function chromeLaunchArgs(launch: LaunchOptions) {
  return [
    `--remote-debugging-port=${launch.port}`,
    `--user-data-dir=${launch.userDataDir}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-background-mode',
    ...(launch.isolated
      ? []
      : [`--profile-directory=${launch.profile}`, '--restore-last-session']),
    ...displayArgs(launch.headless, launch.os),
    ...optionArgs(launch),
    ...proxyArgs(launch.proxy),
    ...(launch.url ? [launch.url] : []),
  ];
}
