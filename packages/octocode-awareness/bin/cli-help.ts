import { hooksInstallUsage } from '../src/hooks-install-specs.js';
import { cliAllowedFlags } from '../src/schema/cli-contract.js';
import { commandIndex } from '../src/schema/command-catalog.js';
import { COMMAND_DISPLAY, COMMAND_EXAMPLE, COMMAND_TO_SCHEMA, HELP, HELP_COMPACT, ROUTE_EXAMPLE } from './cli-help-data.js';
import { COMMAND_ROUTES, KNOWN_FLAGS, SINGLE_COMMANDS, extractGlobalDb, normalizeToken, selectCommand } from './cli-routing.js';

export const COMMAND_HELP: Record<string, string> = {
  'tell-memory': `usage: npx @octocodeai/octocode-awareness memory record --agent-id <id> --task-context <text> --observation <text> --importance <1-10> [--label <l>] [--tag <t>]... [--reference <r>]... [--file <p>]... [--supersedes <id>]... [--allow-similar]
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>]
lifecycle: [--valid-from <iso>] [--valid-to <iso>] [--failure-signature <key>]
example: npx @octocodeai/octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact
note: unknown --label values hard-error
note: --supersedes atomically records a replacement and preserves the replaced row as history
schema: npx @octocodeai/octocode-awareness schema json-schema memory_record --compact`,
  'get-memory': `usage: npx @octocodeai/octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only] [--all-workspaces]
rank: [--smart] [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain]
output: lean/truncated by default; --full restores full memory rows
scope: default = this-workspace + truly-global; --strict-scope = exactly this-workspace; --global-only = only truly-global (all-NULL provenance); --all-workspaces = search across all workspaces (skip workspace_path scoping)
example: npx @octocodeai/octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
schema: npx @octocodeai/octocode-awareness schema json-schema memory_recall --compact`,
  'memory-archive': `usage: npx @octocodeai/octocode-awareness memory archive --memory-id <id>... [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--dry-run]
example: npx @octocodeai/octocode-awareness memory archive --memory-id mem_123 --dry-run --compact
note: reversible archive hides ACTIVE recall while preserving the row; preview first
schema: npx @octocodeai/octocode-awareness schema json-schema memory_lifecycle --compact`,
  'memory-restore': `usage: npx @octocodeai/octocode-awareness memory restore --memory-id <id>... [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--dry-run]
example: npx @octocodeai/octocode-awareness memory restore --memory-id mem_123 --dry-run --compact
note: restores archived rows only; replacement history with superseded_by is never revived
schema: npx @octocodeai/octocode-awareness schema json-schema memory_lifecycle --compact`,
  'forget': `usage: npx @octocodeai/octocode-awareness memory forget (--memory-id <id>... | --tag <t>... | --before <iso> | --max-importance <n>) [--workspace <p>] [--dry-run]
example: npx @octocodeai/octocode-awareness memory forget --memory-id mem_123 --dry-run --compact
note: hard deletion is irreversible; prefer archive for reversible cleanup and always preview broad selectors
schema: npx @octocodeai/octocode-awareness schema json-schema forget_memory --compact`,
  'refine-set': `usage: npx @octocodeai/octocode-awareness refinement set [create: --agent-id <id> --reasoning <t> --remember <t> --workspace <p> | update: --refinement-id <id> --state open|ongoing|done] [--quality good|bad|handoff|instructions] [--agent-id <id>] [--artifact <a>] [--repo <r>] [--ref <r>] [--file <p>]... [--check-receipt <t>]
example: npx @octocodeai/octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact
note: --quality accepts good|bad|handoff|instructions only; create open/ongoing, then close an existing --refinement-id with --state done, --agent-id, and --check-receipt
schema: npx @octocodeai/octocode-awareness schema json-schema refinement --compact`,
  'refine-delete': `usage: npx @octocodeai/octocode-awareness refinement delete --refinement-id <id>... [--workspace <p>] [--artifact <a>] [--dry-run]
example: npx @octocodeai/octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact
note: hard deletion is irreversible; close completed work with refinement set --state done instead
schema: npx @octocodeai/octocode-awareness schema json-schema refine_delete --compact`,
  'digest': `usage: npx @octocodeai/octocode-awareness maintenance digest [--dry-run] [--retention-days <1..3650>] [--refinement-handoff-retention-days <1..3650>] [--refinement-done-retention-days <1..3650>] [--operational-retention-days <1..3650>] [--pressure-age-days <1..3650>]
example: npx @octocodeai/octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact
note: expires ACTIVE memories, purges old SUPERSEDED rows, expired locks, terminal refinements, and terminal standalone runs; reports signal/reference pressure but never prunes signals
schema: npx @octocodeai/octocode-awareness schema json-schema digest --compact`,
  'pre-flight-intent': `usage: npx @octocodeai/octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--run-id <claimed-run>] [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--ttl-minutes <n>] [--wait-seconds <n>]
example: npx @octocodeai/octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
note: lock acquire is exclusive protection for sensitive/non-mergeable work; ordinary mergeable edits use work start
note: existing live presence blocks exclusive acquire; coordinate, wait, switch, or prune only expired protection
note: --run-id attaches exclusive protection to a claimed task run
note: export OCTOCODE_AGENT_ID for CLI+hooks; --strict-agent-id / OCTOCODE_STRICT_AGENT_ID=1 hard-fails when missing
schema: npx @octocodeai/octocode-awareness schema json-schema lock_acquire --compact`,
  'agent-signal': `usage: npx @octocodeai/octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  npx @octocodeai/octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact
  npx @octocodeai/octocode-awareness signal list --agent-id agent --workspace "$PWD" --format hook --compact
  npx @octocodeai/octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  npx @octocodeai/octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
list options: [--limit <n>] [--all|--unread-only] [--mark-read] [--include-bodies] [--format json|hook]
note: --format hook returns the notify briefing shape used by host hooks (list only)
schema: npx @octocodeai/octocode-awareness schema json-schema agent_signal --compact`,
  'verify': `usage: npx @octocodeai/octocode-awareness verify mark (--run-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: npx @octocodeai/octocode-awareness verify mark --agent-id agent --run-id run_123 --message "yarn test passed" --compact
note: prefer explicit --run-id; scope deliberate --all-pending use with --workspace
schema: npx @octocodeai/octocode-awareness schema json-schema verify --compact`,
  'verify audit': `usage: npx @octocodeai/octocode-awareness verify audit [--agent-id <id>] [--workspace <repo>] [--stale-hours <n>]
example: npx @octocodeai/octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact
schema: npx @octocodeai/octocode-awareness schema json-schema verify_audit --compact`,
  'reflect': `usage: npx @octocodeai/octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--worked <t>] [--didnt-work <t>] [--judgment-note <t>] [--lesson <t>] [--fix-repo <t>] [--fix-harness <t>] [--fix-instructions <t>] [--fix-file <p>]... [--failure-signature <s>] [--eval-failure-json <json>]... [--duo] [--allow-similar] [--importance <1..10>] [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>]
example: npx @octocodeai/octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
note: --outcome must be worked|partial|failed; unknown values hard-error
note: --fix-repo → repo-code refinement; --fix-harness → skill/tooling; --fix-instructions → feedback to the human instruction author (see reflect developer-review); refinement --quality values are good|bad|handoff|instructions
schema: npx @octocodeai/octocode-awareness schema json-schema reflect --compact`,
  'developer-review': `usage: npx @octocodeai/octocode-awareness reflect developer-review [--workspace <repo>] [--state open|ongoing|done]... [--format json|markdown] [--limit <n>]
example: npx @octocodeai/octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact
note: reads agent feedback on the instructions themselves (from reflect record --fix-instructions); use --format markdown for an explicit export`,
  'wait-for-lock': `usage: npx @octocodeai/octocode-awareness lock wait [options]
flags: --agent-id --target-file --file --workspace --artifact --wait-seconds --retry-interval
example: npx @octocodeai/octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact
note: waits for other agents' exclusive lock rows only; advisory work presence may still exist
note: after a clear wait, run work show --workspace "$PWD" --file <path> before lock acquire or editing
note: exit 2 means timeout/conflict; do not treat expiry or clear wait as verification
schema: npx @octocodeai/octocode-awareness schema json-schema lock_wait --compact`,
  'query': `usage: npx @octocodeai/octocode-awareness query <all|repo-profile|memories|gotchas|lessons|plans|tasks|runs|locks|agents|signals|refinements|files|activity|workboard|developer-review> [--query <text>] [--limit <1..500>] [--workspace <repo>] [--artifact <a>] [--repo <r>] [--ref <r>] [--agent-id <id>] [--state <s>]... [--label <l>]... [--file <p>] [--since <iso>] [--include-bodies] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  npx @octocodeai/octocode-awareness query files --workspace "$PWD" --format table --limit 50
  npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact
  npx @octocodeai/octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
note: files/memories expose missing file references as file_exists, missing_file, missing_references, and stale_file_refs workboard reasons
schema: npx @octocodeai/octocode-awareness schema json-schema query --compact`,
  'attend': `usage: npx @octocodeai/octocode-awareness attend [--workspace <repo>] [--query <text>] [--agent-id <id>] [--file <p>]... [--limit <n>] [--include-bodies] [--explain-organ]
example: npx @octocodeai/octocode-awareness attend --query "current task" --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --compact
note: pass --agent-id (or OCTOCODE_AGENT_ID) so next routes owned Verify/Claimed before generic evidence
schema: npx @octocodeai/octocode-awareness schema json-schema attend --compact`,
  'docs-catalog': `usage: npx @octocodeai/octocode-awareness docs list|show [name] [--full]
examples:
  npx @octocodeai/octocode-awareness docs list --compact
  npx @octocodeai/octocode-awareness docs show agent-cheatsheet
  npx @octocodeai/octocode-awareness docs show agent-cheatsheet --compact  # JSON only
schema: npx @octocodeai/octocode-awareness schema json-schema docs_catalog --compact`,
  'skill-install': `usage: npx @octocodeai/octocode-awareness skill install --platform <shared|codex|codex-native|claude|claude-desktop|cursor|opencode|pi|copilot|gemini> (--global | --project-dir <path>) [--dry-run] [--force]
examples:
  npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run
  npx @octocodeai/octocode-awareness skill install --platform pi --global --dry-run
note: preview first; run again without --dry-run only after approval. Existing identical installs are unchanged; differing destinations require --force.
schema: none`, 
  'plan-command': `usage: npx @octocodeai/octocode-awareness plan create|list|show|join|doc|status [options]
create: --name <text> --objective <text> --lead-agent-id <id> --workspace <repo> [--artifact <name>]
list: [--workspace <repo>] [--status <status>] [--limit <1-200>] [--full]
show/join/doc/status: --plan-id <id>; join also --agent-id <id>; doc uses --agent-id <member> --path docs/NOTE.md --title <text>; status uses --agent-id <lead> --status DRAFT|ACTIVE|PAUSED|COMPLETED|CANCELLED
example: npx @octocodeai/octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact
schema: npx @octocodeai/octocode-awareness schema json-schema plan --compact`,
  'task-command': `usage: npx @octocodeai/octocode-awareness task create|list|ready|show|claim|heartbeat|submit|release|retry|depend [options]
create: --plan-id <id> --title <text> --reasoning <text> --acceptance <text> --path <workspace-relative>... --agent-id <id> [--depends-on <task-id>]... [--priority <-1000..1000>] [--lease-minutes <1..60>] [--test-plan <text>]
list/ready: [--plan-id <id>] [--workspace <repo>] [--status <s>] [--limit <1-200>] [--full]
show: --task-id <id>
claim: --task-id <id> --agent-id <id>; or --next --plan-id <id> --agent-id <id>. Returns run_id for lock/submit/verify; exit 2 only when another live claimant owns it.
heartbeat/submit/release: --task-id <id> --run-id <id> --agent-id <id>; submit optionally --message <text>; release optionally --blocked-reason <text>
retry: --task-id <failed-task> --agent-id <id> [--message <text>]
depend: --task-id <id> --depends-on <task-id>...
example: npx @octocodeai/octocode-awareness task ready --plan-id plan_123 --compact
schema: npx @octocodeai/octocode-awareness schema json-schema task --compact`,
  'work-command': `usage: npx @octocodeai/octocode-awareness work start|touch|end|list|show [options]
start new WORK: --file <path>... --agent-id <id> [--workspace <repo>] --rationale <text> --test-plan <text> [--exclusive]
attach task run: --run-id <claimed-task-run> --file <path>... --agent-id <id> [--exclusive]
touch: --run-id <id> --agent-id <id> [--file <path>]... refreshes already-declared active files; use start --run-id to add files
end: --run-id <id> --agent-id <id> [--file <path>]...
list: [--workspace <repo>] [--agent-id <id>] [--run-id <id>] [--all] [--limit <1-200>] [--full]
show: --workspace <repo> --file <path> [--all] [--limit <1-200>] [--full]
example: npx @octocodeai/octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact
note: ordinary edits are advisory and may overlap; use --exclusive only for unsafe/non-mergeable sensitive work
note: on overlap, inspect work show and signal if edits interact; never surprise active peers with a lock
schema: npx @octocodeai/octocode-awareness schema json-schema work --compact`,
  'hook-run': `usage: octocode-awareness hook run <pre-edit|post-edit|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json
payload: host JSON on stdin; common fields are cwd/workspace, session_id, tool_name, and tool_input/path
store: hook run intentionally rejects --db; payload workspace selects <workspace>/.octocode/awareness.sqlite3`,
  'hooks-install': hooksInstallUsage(),
  'schema': `usage: npx @octocodeai/octocode-awareness schema commands|entities|list|command <noun> [action]|json-schema <name>|example <name>|validate <name> <json-file|->
examples:
  npx @octocodeai/octocode-awareness schema commands --compact
  npx @octocodeai/octocode-awareness schema entities --compact
  npx @octocodeai/octocode-awareness schema command memory recall --compact
  npx @octocodeai/octocode-awareness schema json-schema query --compact`,
  'init': `usage: npx @octocodeai/octocode-awareness maintenance init [--db <path>]
example: npx @octocodeai/octocode-awareness maintenance init --db-scope global --compact`,
  'self-test': `usage: npx @octocodeai/octocode-awareness maintenance self-test
example: npx @octocodeai/octocode-awareness maintenance self-test --compact`,
  'awareness-config': `usage: npx @octocodeai/octocode-awareness config show|init|validate [options]
init flags: --hooks <true|false> --notifications <true|false> --verification-gate <true|false> --session-capture <true|false> --maintenance-reminders <true|false>
note: config init requires every answer and refuses to overwrite; config validate checks the resolved Awareness config
schema: npx @octocodeai/octocode-awareness schema json-schema awareness_config --compact`,
};

const FOCUSED_GLOBAL_FLAGS = ['db', 'db_scope', 'compact', 'help'] as const;

export function hyphenFlag(flag: string): string {
  return `--${flag.replace(/_/g, '-')}`;
}

export function helpFor(command: string | null, options: { compact?: boolean; routeKey?: string } = {}): string {
  if (!command && options.routeKey?.startsWith('noun:')) {
    const noun = options.routeKey.slice('noun:'.length);
    const actions = [...new Set(Object.keys(COMMAND_ROUTES)
      .filter((route) => route.startsWith(`${noun} `))
      .map((route) => route.slice(noun.length + 1)))];
    const actionList = actions.join('|');
    const firstRoute = actions.length > 0 ? `${noun} ${actions[0]}` : noun;
    return [
      `usage: npx @octocodeai/octocode-awareness ${noun}${actionList ? ` ${actionList}` : ''} [options]`,
      `details: npx @octocodeai/octocode-awareness ${firstRoute} --help`,
      `map: npx @octocodeai/octocode-awareness schema command ${firstRoute} --compact`,
    ].join('\n');
  }
  if (!command) return options.compact ? HELP_COMPACT : HELP;
  const normalized = command.replace(/_/g, '-');
  const catalog = options.routeKey
    ? commandIndex.find((entry) => entry.command === options.routeKey)
    : undefined;
  const actionFlags = catalog ? cliAllowedFlags(catalog.command) : undefined;
  const flags = actionFlags
    ? [...new Set([...actionFlags, ...FOCUSED_GLOBAL_FLAGS])]
    : KNOWN_FLAGS[normalized];
  if (!flags) return HELP;
  const schema = catalog?.schema ?? COMMAND_TO_SCHEMA[normalized] ?? null;
  const display = options.routeKey ?? COMMAND_DISPLAY[normalized] ?? normalized;
  const example = catalog?.example
    ?? (options.routeKey ? ROUTE_EXAMPLE[options.routeKey] : undefined)
    ?? COMMAND_EXAMPLE[normalized];
  if (options.compact) {
    return [
      `usage: npx @octocodeai/octocode-awareness ${display} [options]`,
      `flags: ${flags.map(hyphenFlag).join(' ')}`,
      schema ? `schema: ${schema}` : 'schema: none',
      `example: ${example ?? `npx @octocodeai/octocode-awareness ${display}`}`,
    ].join('\n').trimEnd();
  }
  if (options.routeKey && COMMAND_HELP[options.routeKey]) return COMMAND_HELP[options.routeKey]!;
  if (catalog?.command === COMMAND_DISPLAY[normalized] && COMMAND_HELP[normalized]) return COMMAND_HELP[normalized]!;
  if (catalog && cliAllowedFlags(catalog.command)) {
    return [
      `usage: npx @octocodeai/octocode-awareness ${display} [options]`,
      `flags: ${flags.map(hyphenFlag).join(' ')}`,
      schema ? `schema: npx @octocodeai/octocode-awareness schema json-schema ${schema} --compact` : 'schema: none',
      example ? `example: ${example}` : '',
    ].join('\n').trimEnd();
  }
  if (COMMAND_HELP[normalized]) return COMMAND_HELP[normalized]!;
  return [
    `usage: npx @octocodeai/octocode-awareness ${display} [options]`,
    `flags: ${flags.map(hyphenFlag).join(' ')}`,
    schema ? `schema: npx @octocodeai/octocode-awareness schema json-schema ${schema} --compact` : 'schema: none',
    example ? `example: ${example}` : '',
  ].join('\n').trimEnd();
}

export function commandFromHelpArgv(argv: string[]): { command: string | null; routeKey?: string } {
  const withoutHelp = argv.filter((arg) => arg !== '--help' && arg !== '-h' && arg !== '--compact');
  const filtered = extractGlobalDb(withoutHelp).filtered;
  const [firstRaw, secondRaw] = filtered;
  const first = normalizeToken(firstRaw);
  const second = normalizeToken(secondRaw);
  let routeKey: string | undefined;
  if (first === 'hook' && second === 'run') routeKey = 'hook run';
  else if (first === 'hooks' && second && ['install', 'check', 'remove'].includes(second)) routeKey = `hooks ${second}`;
  else if (first === 'schema' && second && ['commands', 'list', 'json-schema', 'example', 'validate'].includes(second)) routeKey = `schema ${second}`;
  else if (first && second && COMMAND_ROUTES[`${first} ${second}`]) routeKey = `${first} ${second}`;
  else if (first && SINGLE_COMMANDS.has(first)) routeKey = first;
  const command = selectCommand(filtered).command ?? null;
  if (!command && first && !second && Object.keys(COMMAND_ROUTES).some((route) => route.startsWith(`${first} `))) {
    routeKey = `noun:${first}`;
  }
  return { command, routeKey };
}
