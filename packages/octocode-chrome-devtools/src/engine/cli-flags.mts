// Strict noninteractive flag validation for saved-evidence readers.
export function validateFlags(
  args: string[],
  values: string[],
  booleans: string[] = ['--help', '-h'],
  maxPositionals = 0
) {
  const seen = new Set();
  const positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const flag = args[i];
    if (!flag.startsWith('-') && positional.length < maxPositionals) {
      positional.push(flag);
      continue;
    }
    if (!values.includes(flag) && !booleans.includes(flag))
      throw new Error(`Unknown option ${flag}; use --help`);
    if (seen.has(flag)) throw new Error(`Duplicate option ${flag}`);
    seen.add(flag);
    if (values.includes(flag)) {
      if (args[i + 1] === undefined || args[i + 1].startsWith('--'))
        throw new Error(`${flag} needs a value`);
      i++;
    }
  }
  return positional;
}

/**
 * Value and presence readers over one argv. A flag with no value, or an empty one, reads as `def`,
 * so `--page ""` falls back to the default instead of becoming NaN downstream.
 */
export function flagArgs(argv: string[]) {
  function getArg(flag: string): string | undefined;
  function getArg(flag: string, def: string): string;
  function getArg(flag: string, def?: string): string | undefined {
    const i = argv.indexOf(flag);
    return (i === -1 ? undefined : argv[i + 1]) || def;
  }
  const hasFlag = (flag: string) => argv.includes(flag);
  /** A case-insensitive filter value: lowercase, empty when the flag is absent. */
  const getLowerArg = (flag: string) => getArg(flag, '').toLowerCase();
  return { getArg, hasFlag, getLowerArg };
}
