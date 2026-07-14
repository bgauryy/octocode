/**
 * octocode-skills — Octocode skills distribution CLI
 *
 * ─── QUICK START ─────────────────────────────────────────────────────────────
 *
 *   octocode-skills list                     see all skills + install status
 *   octocode-skills install --all            install everything (override by default)
 *   octocode-skills install <name> --platform pi    install + link to pi
 *   octocode-skills check                    verify all installations
 *   octocode-skills info <name>              read full SKILL.md
 *
 * ─── AGENT QUICK-START ───────────────────────────────────────────────────────
 *
 *   octocode-skills list --json              discover available skills + status
 *   octocode-skills install <name> --json    install and parse result
 *   octocode-skills check --json             verify installations (JSON)
 *   octocode-skills info <name> --json       read full SKILL.md as JSON
 *
 *   All commands exit 0 on success, non-zero on failure.
 *   All commands support --json for machine-readable output.
 *
 * ─── OVERRIDE vs KEEP ────────────────────────────────────────────────────────
 *
 *   Install OVERRIDES existing by default (always gets the latest bundled copy).
 *   Pass --keep to preserve an existing installation instead of overwriting it.
 *
 *   octocode-skills install octocode-research          # override (default)
 *   octocode-skills install octocode-research --keep   # skip if already installed
 *
 * ─── INSTALLATION MODEL ───────────────────────────────────────────────────────
 *
 *   Step 1 — Canonical home (always, unless --path):
 *     ~/.octocode/skills/<name>/        ← one authoritative copy per skill
 *
 *   Step 2 — Platform link (--platform):
 *     ~/.pi/agent/skills/<name>    →  ~/.octocode/skills/<name>    (pi)
 *     ~/.cursor/skills/<name>      →  ~/.octocode/skills/<name>    (cursor)
 *     …
 *
 *   Step 3 — Workspace link (--workspace):
 *     <cwd>/.agents/skills/<name>  →  ~/.octocode/skills/<name>
 *
 *   Step 4 — Custom path (--path, skips home):
 *     <dir>/<name>/                ← direct copy or symlink
 *
 * ─────────────────────────────────────────────────────────────────────────────
 */

// Injected at build time by esbuild define; tsc trusts the ambient declaration.
declare const __PKG_VERSION__: string;

import { runList } from './commands/list.js';
import { runInstall } from './commands/install.js';
import { runRemove } from './commands/remove.js';
import { runInfo } from './commands/info.js';
import { runCheck } from './commands/check.js';
import { bold, dim, cyan } from './utils/colors.js';

// ─── Minimal arg parser ────────────────────────────────────────────────────────

interface ParsedArgs {
  command: string | null;
  positional: string[];
  flags: Record<string, string | boolean>;
}

function parseArgs(argv: string[]): ParsedArgs {
  const args = argv.slice(2);
  const positional: string[] = [];
  const flags: Record<string, string | boolean> = {};

  let i = 0;
  while (i < args.length) {
    const arg = args[i]!;

    if (arg.startsWith('--')) {
      const key = arg.slice(2);
      const next = args[i + 1];
      // treat next as value only if it doesn't start with -
      if (next !== undefined && !next.startsWith('-')) {
        flags[key] = next;
        i += 2;
      } else {
        flags[key] = true;
        i += 1;
      }
    } else if (arg === '-h') {
      flags['help'] = true;
      i += 1;
    } else {
      positional.push(arg);
      i += 1;
    }
  }

  const [command = null, ...rest] = positional;
  return { command, positional: rest, flags };
}

// ─── Help ─────────────────────────────────────────────────────────────────────

function printHelp(): void {
  console.log(`
${bold('octocode-skills')} — Octocode skills distribution

${bold('Usage')}
  octocode-skills <command> [options]

${bold('Commands')}
  list                    List all skills with install status
  install <name>...       Install one or more skills  ${dim('(override by default)')}
  remove  <name>...       Remove a skill — home copy + all platform links
  check  [<name>...]      Verify installations — home, platform links, broken symlinks
  info   <name>           Show full SKILL.md content

${bold('Install options')}
  --all                   Install all bundled skills
  --platform <p>          Link into platform dir  ${dim('(comma-sep: pi | cursor | claude | claude-desktop | codex | opencode | copilot | gemini | common | all)')}
  --workspace, --repo     Also link into <cwd>/.agents/skills/
  --path <dir>            Install directly to <dir>  ${dim('(skips home)')}
  --mode copy|symlink|hybrid  ${dim('[default: symlink · hybrid = copy for claude]')}
  --keep                  Preserve existing  ${dim('[default: override]')}
  --dry-run               Preview without writing

${bold('Remove options')}
  --all                   Remove all installed skills
  --platform <p>          Remove only the specified platform link(s)  ${dim('(home kept)')}
  --dry-run               Preview without deleting

${bold('Check options')}
  --platform <p>          Check specific platforms only
  --workspace             Also check <cwd>/.agents/skills
  --fix                   Re-install missing/broken locations automatically
  --no-env                Skip env param checks

${bold('Global flags')}
  --json                  Machine-readable JSON output  ${dim('(all commands)')}
  --help, -h              Show this help
  --version               Show version

${bold('Examples')}
  octocode-skills list --json
  octocode-skills install --all --platform pi,cursor
  octocode-skills install octocode-research --workspace --keep
  octocode-skills remove octocode-research --platform pi
  octocode-skills check --fix
  octocode-skills info octocode-research
`);
}

// ─── Entry point ──────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const { command, positional, flags } = parseArgs(process.argv);

  const json = Boolean(flags['json']);
  const help = Boolean(flags['help'] ?? flags['h']);

  if (flags['version']) {
    console.log(__PKG_VERSION__);
    return;
  }

  if (help || !command) {
    printHelp();
    return;
  }

  switch (command) {
    // ── list ────────────────────────────────────────────────────────────────
    case 'list':
      runList({ json });
      break;

    // ── info ────────────────────────────────────────────────────────────────
    case 'info': {
      const skillName = positional[0];
      if (!skillName) {
        console.error('Usage: octocode-skills info <skill-name>');
        process.exitCode = 1;
        return;
      }
      runInfo(skillName, { json });
      break;
    }

    // ── check ───────────────────────────────────────────────────────────────
    case 'check': {
      const names = positional.filter((a) => !a.startsWith('-'));
      const rawPlatform = flags['platform'];
      const platform = typeof rawPlatform === 'string' ? rawPlatform : null;

      runCheck({
        names,
        platform,
        workspace: Boolean(flags['workspace'] ?? flags['repo']),
        fix: Boolean(flags['fix']),
        noEnv: Boolean(flags['no-env']),
        json,
      });
      break;
    }

    // ── install ─────────────────────────────────────────────────────────────
    case 'install': {
      const skillNames = positional.filter((a) => !a.startsWith('-'));
      const rawMode = flags['mode'];
      const mode = rawMode === 'copy' ? 'copy' : rawMode === 'hybrid' ? 'hybrid' : 'symlink';
      const rawPlatform = flags['platform'];
      const platform = typeof rawPlatform === 'string' ? rawPlatform : null;
      const rawPath = flags['path'];
      const customPath = typeof rawPath === 'string' ? rawPath : null;

      runInstall(skillNames, {
        all: Boolean(flags['all']),
        platform,
        workspace: Boolean(flags['workspace'] ?? flags['repo']),
        customPath,
        mode,
        // --keep = preserve existing; default is override
        keep: Boolean(flags['keep']),
        dryRun: Boolean(flags['dry-run']),
        json,
      });
      break;
    }

    // ── remove ─────────────────────────────────────────────────────────────
    case 'remove': {
      const skillNames = positional.filter((a) => !a.startsWith('-'));
      const rawPlatform = flags['platform'];
      const platform = typeof rawPlatform === 'string' ? rawPlatform : null;

      runRemove(skillNames, {
        all: Boolean(flags['all']),
        platform,
        dryRun: Boolean(flags['dry-run']),
        json,
      });
      break;
    }

    case 'help':
      printHelp();
      break;

    default:
      if (json) {
        console.log(JSON.stringify({ success: false, error: `Unknown command: "${command}"` }));
      } else {
        console.error(`\n  Unknown command: "${command}"`);
        console.error(`  Run ${cyan('octocode-skills --help')} for usage.\n`);
      }
      process.exitCode = 1;
  }
}

main().catch((err: unknown) => {
  const msg = err instanceof Error ? err.message : String(err);
  console.error(`Fatal: ${msg}`);
  process.exitCode = 1;
});
