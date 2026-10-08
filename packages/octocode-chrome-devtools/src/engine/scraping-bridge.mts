import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

export function resolveScrapingTool(
  tool: string,
  args: string[],
  missing: string
) {
  const positions = args.flatMap((value, index) =>
      value === '--scraping-skill-dir' ? [index] : []
    ),
    index = positions[0] ?? -1;
  if (
    positions.length > 1 ||
    (index >= 0 && (!args[index + 1] || args[index + 1].startsWith('--')))
  ) {
    console.error(
      JSON.stringify({
        ok: false,
        code: 'INVALID_ARGUMENT',
        error: '--scraping-skill-dir requires a directory',
      })
    );
    process.exit(2);
  }
  const directory =
    index >= 0
      ? resolve(args[index + 1])
      : resolve(import.meta.dirname, '../../../..', 'skills/octocode-scraping');
  const target = join(directory, 'scripts', tool + '.mjs');
  if (!existsSync(target)) {
    console.error(
      JSON.stringify({
        ok: false,
        code: 'OPTIONAL_DEPENDENCY_MISSING',
        error: missing,
      })
    );
    process.exit(1);
  }
  return {
    target,
    args:
      index >= 0
        ? args.filter((_, at) => at !== index && at !== index + 1)
        : args,
  };
}

export function runScrapingTool(
  tool: string,
  args: string[],
  help: string,
  missing: string
) {
  if (args.includes('--help') || args.includes('-h')) {
    console.log(help);
    return 0;
  }
  const resolved = resolveScrapingTool(tool, args, missing);
  return (
    spawnSync(process.execPath, [resolved.target, ...resolved.args], {
      cwd: process.cwd(),
      encoding: 'utf8',
      stdio: 'inherit',
    }).status ?? 1
  );
}
