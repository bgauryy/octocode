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
