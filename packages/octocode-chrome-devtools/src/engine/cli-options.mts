// Core CLI option parsing: flags, JSON inputs, and the connection flags forwarded to the runner.

/** Every CLI input error is thrown; the CLI prints it and exits 2. */
export const fail = (message: string): never => {
  throw new Error(message);
};

/** Parsed options: a value option maps to its text, a boolean option to true. */
export type CliOptions = Record<string, string | true | undefined>;

/** True when `key` is an own key of `record`, which narrows it to that record's keys. */
export function isOwnKey<T extends object>(
  record: T,
  key: string | undefined
): key is Extract<keyof T, string> {
  return key !== undefined && Object.hasOwn(record, key);
}

/** A value option's text; parse() never stores a boolean under a value option. */
export function optionValue(opts: CliOptions, key: string) {
  const value = opts[key];
  return typeof value === 'string' ? value : undefined;
}

export const json = (text: string, label: string): unknown => {
  try {
    return JSON.parse(text);
  } catch {
    return fail(`${label} must be valid JSON`);
  }
};

/** Splits args into known options (values and booleans) and positionals; unknown or repeated options fail. */
export function parse(args: string[], values: string[], booleans: string[]) {
  const opts: CliOptions = {},
    positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const key = args[i];
    if (!key.startsWith('--')) {
      positional.push(key);
      continue;
    }
    if (!values.includes(key) && !booleans.includes(key))
      fail(`Unknown option ${key}; use --help`);
    if (Object.hasOwn(opts, key)) fail(`Duplicate option ${key}`);
    if (booleans.includes(key)) opts[key] = true;
    else {
      if (!args[i + 1] || args[i + 1].startsWith('--'))
        fail(`${key} needs a value`);
      opts[key] = args[++i];
    }
  }
  return { opts, positional };
}

export const commonValues = [
  '--port',
  '--target',
  '--target-url',
  '--target-type',
  '--new-tab',
  '--timeout',
  '--script-timeout',
];
export const commonBooleans = [
  '--browser',
  '--keep-tab',
  '--close-tab',
  '--no-reload',
  '--stealth',
  '--no-stealth',
  '--verbose',
  '--dry-run',
];

function validateNumbers(opts: CliOptions) {
  for (const key of ['--port', '--timeout', '--script-timeout'])
    if (
      opts[key] !== undefined &&
      (!/^\d+$/.test(String(opts[key])) ||
        !Number.isSafeInteger(Number(opts[key])) ||
        Number(opts[key]) < 1 ||
        (key === '--port' && Number(opts[key]) > 65535))
    )
      fail(
        `${key} needs a positive integer${key === '--port' ? ' <= 65535' : ''}`
      );
}

function validateSelection(opts: CliOptions) {
  if (opts['--keep-tab'] && opts['--close-tab'])
    fail('Use one of --keep-tab and --close-tab');
  if (opts['--stealth'] && opts['--no-stealth'])
    fail('Use one of --stealth and --no-stealth');
  const selectors = [
    '--target',
    '--target-url',
    '--new-tab',
    '--browser',
  ].filter(key => opts[key]);
  if (
    selectors.length > 1 ||
    (opts['--browser'] && (opts['--target-type'] || opts['--stealth']))
  )
    fail(
      'Use one target selector; --browser cannot combine with page selection or stealth'
    );
  if (opts['--new-tab'] && opts['--target-type'])
    fail('--new-tab cannot combine with --target-type');
}

/** Validated connection options as runner arguments: strict target, no reload, tabs kept unless --close-tab. */
export function runnerArgs(opts: CliOptions) {
  validateNumbers(opts);
  validateSelection(opts);
  const args: string[] = [
    '--strict-target',
    '--no-reload',
    ...(opts['--close-tab'] ? [] : ['--keep-tab']),
  ];
  for (const key of [...commonValues, ...commonBooleans])
    if (
      opts[key] &&
      !['--dry-run', '--close-tab', '--keep-tab', '--no-reload'].includes(key)
    )
      args.push(
        key,
        ...(commonValues.includes(key) ? [String(opts[key])] : [])
      );
  return args;
}
