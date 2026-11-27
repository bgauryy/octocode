// How a fresh Chrome is launched: the proxy from flags or config files, and the command-line flags.
import { existsSync, readFileSync } from 'fs';
import { join } from 'path';
import { chromeFlagDenylist } from './chrome-contract.mjs';
import { errorMessage } from './error-message.mjs';
import { octocodeWorkspace } from './engine-io.mjs';
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
  const cwdConfig = join(octocodeWorkspace(), 'chrome-devtools.json');
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
    // A headless profile is throwaway and has nobody to answer a prompt: without a usable
    // login keychain (a service, a sandboxed HOME) macOS Chrome holds its first request on it.
    ...(os === 'darwin' ? ['--use-mock-keychain'] : []),
    ...(os === 'linux' ? ['--disable-gpu'] : []),
  ];
}

/**
 * Chrome's sandbox refuses to start as root (containers, CI) and on kernels without user
 * namespaces; it stays on unless the caller opts out with noSandbox or runs as root on Linux.
 */
export function sandboxArgs(os: string, noSandbox: boolean | undefined) {
  const root = os === 'linux' && process.getuid?.() === 0;
  return noSandbox || root ? ['--no-sandbox', '--disable-setuid-sandbox'] : [];
}

/** The launch failure text when Chrome exits at start because it has no usable sandbox. */
export const NO_SANDBOX_HINT =
  'Chrome found no usable sandbox (typical in containers or without user namespaces). Retry with noSandbox: true (open --no-sandbox); that turns the browser sandbox off, so open only trusted pages.';

/** Whether Chrome's start-up output says the sandbox could not be used. */
export const isNoSandboxFailure = (output: string) =>
  /no usable sandbox/i.test(output);

function proxyArgs(proxy: ProxySettings) {
  return [
    ...(proxy.server ? [`--proxy-server=${proxy.server}`] : []),
    ...(proxy.bypassList ? [`--proxy-bypass-list=${proxy.bypassList}`] : []),
    ...(proxy.pacUrl && !proxy.server
      ? [`--proxy-pac-url=${proxy.pacUrl}`]
      : []),
  ];
}

/**
 * Features every fresh launch enables. WebMCP (Chrome 150+) exposes `document.modelContext` to
 * pages and the CDP `WebMCP` domain, so page-declared tools can be listed and invoked; builds
 * without it ignore the name.
 */
const DEFAULT_FEATURES = ['WebMCP'];

/** The default features plus the requested CSV, each once, as one `--enable-features` value. */
export function launchFeatures(requested: string) {
  const extra = requested.split(',').map(feature => feature.trim());
  return [...new Set([...DEFAULT_FEATURES, ...extra.filter(Boolean)])].join(
    ','
  );
}

/** A comma-separated feature list as trimmed, non-empty, unique names. */
const featureNames = (csv: string) => [
  ...new Set(
    csv
      .split(',')
      .map(name => name.trim())
      .filter(Boolean)
  ),
];

const CHROME_FLAG_SHAPE = /^--[a-z][a-z0-9-]*(=[^\s,]*)?$/;
const flagName = (entry: string) => entry.split('=')[0] ?? entry;

/** The refusal text for one typed chrome flag, or null when it is allowed (denylist from the shared contract). */
export function chromeFlagRefusal(entry: string): string | null {
  if (!CHROME_FLAG_SHAPE.test(entry))
    return `${entry} is not a valid Chrome flag: use --name or --name=value, no spaces or commas`;
  const name = flagName(entry);
  const denied = chromeFlagDenylist.find(rule =>
    rule.prefix ? name.startsWith(rule.flag) : name === rule.flag
  );
  return denied ? `${name} is refused: ${denied.reason}` : null;
}

/**
 * The typed chrome flags as unique entries. Throws a message naming the flag and reason for a
 * malformed or denylisted entry, or for one name given twice with different values.
 */
export function validateChromeFlags(flags: readonly string[]): string[] {
  const seen = new Map<string, string>();
  for (const entry of flags) {
    const refusal = chromeFlagRefusal(entry);
    if (refusal) throw new Error(refusal);
    const name = flagName(entry);
    const earlier = seen.get(name);
    if (earlier !== undefined && earlier !== entry)
      throw new Error(`${name} is given twice with different values`);
    seen.set(name, entry);
  }
  return [...seen.values()];
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
  /** Requested features; the defaults are always added. */
  enableFeatures: string;
  proxy: ProxySettings;
  url: string;
  /** Features to disable (CSV); one --disable-features switch. */
  disableFeatures?: string;
  /** Blink features to enable (CSV); one --enable-blink-features switch. */
  blinkFeatures?: string;
  /** Extra flags appended after the managed ones; each validated, none may repeat a managed flag. */
  chromeFlags?: readonly string[];
  /** Explicit opt-in to Chrome's --no-sandbox (also applied automatically for root on Linux). */
  noSandbox?: boolean;
};

const switchArg = (name: string, csv: string | undefined) => {
  const names = featureNames(csv ?? '');
  return names.length ? [`${name}=${names.join(',')}`] : [];
};

function optionArgs(launch: LaunchOptions) {
  return [
    ...(launch.userAgent ? [`--user-agent=${launch.userAgent}`] : []),
    ...(launch.windowSize
      ? [`--window-size=${launch.windowSize.replace('x', ',')}`]
      : []),
    `--enable-features=${launchFeatures(launch.enableFeatures)}`,
    ...switchArg('--disable-features', launch.disableFeatures),
    ...switchArg('--enable-blink-features', launch.blinkFeatures),
  ];
}

/** The Chrome command line for a fresh browser on launch.port. */
export function chromeLaunchArgs(launch: LaunchOptions) {
  const managed = managedLaunchArgs(launch);
  const extra = validateChromeFlags(launch.chromeFlags ?? []);
  const managedNames = new Set(managed.map(flagName));
  for (const entry of extra)
    if (managedNames.has(flagName(entry)))
      throw new Error(
        `${flagName(entry)} is refused: the launch already sets it`
      );
  // The launch URL stays last; the extra flags follow every managed flag.
  const url = launch.url ? [launch.url] : [];
  return [...managed, ...extra, ...url];
}

function managedLaunchArgs(launch: LaunchOptions) {
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
    ...sandboxArgs(launch.os, launch.noSandbox),
    ...optionArgs(launch),
    ...proxyArgs(launch.proxy),
  ];
}

/**
 * The extra launch flags a call actually asked for, for the open reply: chrome flags, feature
 * lists, user agent, window size, and proxy presence only (never the server or its credentials).
 */
export function appliedLaunchFlags(launch: {
  chromeFlags: readonly string[];
  enableFeatures: string;
  disableFeatures: string;
  blinkFeatures: string;
  userAgent: string;
  windowSize: string;
  proxyRequested: boolean;
}): { appliedFlags: string[]; proxy?: true } {
  const list = (name: string, csv: string) => {
    const names = featureNames(csv);
    return names.length ? [`${name}=${names.join(',')}`] : [];
  };
  return {
    appliedFlags: [
      ...list('--enable-features', launch.enableFeatures),
      ...list('--disable-features', launch.disableFeatures),
      ...list('--enable-blink-features', launch.blinkFeatures),
      ...(launch.userAgent ? ['--user-agent'] : []),
      ...(launch.windowSize
        ? [`--window-size=${launch.windowSize.replace('x', ',')}`]
        : []),
      ...validateChromeFlags(launch.chromeFlags),
    ],
    ...(launch.proxyRequested ? { proxy: true as const } : {}),
  };
}
