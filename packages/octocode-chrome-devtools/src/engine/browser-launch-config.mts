// How a fresh Chrome is launched: the proxy from flags or config files, and the command-line flags.
import { existsSync, readFileSync } from 'fs';
import { join } from 'path';
import { getOctocodeHome } from './octocode-config.mjs';

type Fail = (message: string) => never;

function readJsonFile(filePath, strict: boolean, { fail, warn }) {
  if (!filePath || !existsSync(filePath)) return null;
  try {
    return JSON.parse(readFileSync(filePath, 'utf8'));
  } catch (jsonErr) {
    if (strict)
      fail(`Invalid JSON in config file: ${filePath} (${jsonErr.message})`);
    warn(
      `[BROWSER] Warning: ignoring invalid JSON config ${filePath}: ${jsonErr.message}`
    );
    return null;
  }
}

const trimmed = value => (typeof value === 'string' ? value.trim() : '');

function normalizeProxyConfig(raw) {
  if (!raw || typeof raw !== 'object') return null;
  return {
    enabled: raw.enabled === undefined ? true : Boolean(raw.enabled),
    server: trimmed(raw.server),
    bypassList: trimmed(raw.bypassList),
    pacUrl: trimmed(raw.pacUrl),
  };
}

const proxyCandidates = json => [
  json?.proxy,
  json?.chromeDevtools?.proxy,
  json?.skills?.['octocode-chrome-devtools']?.proxy,
];

/**
 * The first proxy block in --config, else <cwd>/.octocode/chrome-devtools.json, else the
 * Octocode home config.json. A missing or invalid --config file fails.
 */
export function loadProxyConfig(
  configPath,
  io: { fail: Fail; warn: (line: string) => void }
) {
  const cwdConfig = join(process.cwd(), '.octocode', 'chrome-devtools.json');
  const homeConfig = join(getOctocodeHome(), 'config.json');
  if (configPath && !existsSync(configPath))
    io.fail(`Config file not found: ${configPath}`);
  const candidateFiles = configPath ? [configPath] : [cwdConfig, homeConfig];
  for (const configFile of candidateFiles) {
    const json = readJsonFile(configFile, Boolean(configPath), io);
    if (!json || typeof json !== 'object') continue;
    for (const candidate of proxyCandidates(json)) {
      const normalized = normalizeProxyConfig(candidate);
      if (normalized) return normalized;
    }
  }
  return null;
}

/** Proxy flags win over an enabled proxy config. */
export function effectiveProxy(flags, config) {
  const fromConfig = config?.enabled ? config : null;
  return {
    server: flags.server || (fromConfig ? fromConfig.server : ''),
    bypassList: flags.bypassList || (fromConfig ? fromConfig.bypassList : ''),
    pacUrl: flags.pacUrl || (fromConfig ? fromConfig.pacUrl : ''),
  };
}

function displayArgs(headless, os) {
  if (!headless) return [];
  return [
    '--headless=new',
    '--disable-dev-shm-usage',
    ...(os === 'linux'
      ? ['--disable-gpu', '--no-sandbox', '--disable-setuid-sandbox']
      : []),
  ];
}

function proxyArgs(proxy) {
  return [
    ...(proxy.server ? [`--proxy-server=${proxy.server}`] : []),
    ...(proxy.bypassList ? [`--proxy-bypass-list=${proxy.bypassList}`] : []),
    ...(proxy.pacUrl && !proxy.server
      ? [`--proxy-pac-url=${proxy.pacUrl}`]
      : []),
  ];
}

function optionArgs(launch) {
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
export function chromeLaunchArgs(launch) {
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
