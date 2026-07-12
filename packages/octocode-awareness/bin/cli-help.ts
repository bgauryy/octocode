import { hooksInstallUsage } from '../src/hooks-install.js';
import { COMMAND_DISPLAY, COMMAND_EXAMPLE, COMMAND_TO_SCHEMA, HELP, HELP_COMPACT, ROUTE_EXAMPLE } from './cli-help-data.js';
import { COMMAND_ROUTES, KNOWN_FLAGS, SINGLE_COMMANDS, extractGlobalDb, normalizeToken, selectCommand } from './cli-routing.js';

export const COMMAND_HELP: Record<string, string> = {
  'tell-memory': `usage: octocode-awareness memory record --agent-id <id> --task-context <text> --observation <text> --importance <1-10> [--label <l>] [--tag <t>]... [--reference <r>]... [--file <p>]... [--supersedes <id>]... [--allow-similar]
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>]
lifecycle: [--valid-from <iso>] [--valid-to <iso>] [--failure-signature <key>]
example: octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact
note: unknown --label values hard-error
note: --supersedes atomically records a replacement and preserves the replaced row as history
schema: octocode-awareness schema json-schema memory_record --compact`,
  'get-memory': `usage: octocode-awareness memory recall [options]
filters: [--query <text>] [--limit <n>] [--min-importance <n>] [--label <l>]... [--tag <t>]... [--reference <r>]... [--file <p>]... [--regex <r>]... [--file-regex <r>]...
scope: [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--strict-scope] [--global-only]
rank: [--smart] [--sort smart|score|importance|recent|accessed] [--state ACTIVE|SUPERSEDED]... [--as-of <iso>] [--semantic] [--explain]
output: lean/truncated by default; --full restores full memory rows
example: octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
schema: octocode-awareness schema json-schema memory_recall --compact`,
  'memory-archive': `usage: octocode-awareness memory archive --memory-id <id>... [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--dry-run]
example: octocode-awareness memory archive --memory-id mem_123 --dry-run --compact
note: reversible archive hides ACTIVE recall while preserving the row; preview first
schema: octocode-awareness schema json-schema memory_lifecycle --compact`,
  'memory-restore': `usage: octocode-awareness memory restore --memory-id <id>... [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>] [--dry-run]
example: octocode-awareness memory restore --memory-id mem_123 --dry-run --compact
note: restores archived rows only; replacement history with superseded_by is never revived
schema: octocode-awareness schema json-schema memory_lifecycle --compact`,
  'forget': `usage: octocode-awareness memory forget (--memory-id <id>... | --tag <t>... | --before <iso> | --max-importance <n>) [--workspace <p>] [--dry-run]
example: octocode-awareness memory forget --memory-id mem_123 --dry-run --compact
note: hard deletion is irreversible; prefer archive for reversible cleanup and always preview broad selectors
schema: octocode-awareness schema json-schema forget_memory --compact`,
  'refine-set': `usage: octocode-awareness refinement set [create: --agent-id <id> --reasoning <t> --remember <t> --workspace <p> | update: --refinement-id <id> --state open|ongoing|done] [--quality good|bad|handoff|instructions] [--agent-id <id>] [--artifact <a>] [--repo <r>] [--ref <r>] [--file <p>]... [--check-receipt <t>]
example: octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact
note: --quality accepts good|bad|handoff|instructions only; create open/ongoing, then close an existing --refinement-id with --state done, --agent-id, and --check-receipt
schema: octocode-awareness schema json-schema refinement --compact`,
  'refine-delete': `usage: octocode-awareness refinement delete --refinement-id <id>... [--workspace <p>] [--artifact <a>] [--dry-run]
example: octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact
note: hard deletion is irreversible; close completed work with refinement set --state done instead
schema: octocode-awareness schema json-schema refine_delete --compact`,
  'digest': `usage: octocode-awareness maintenance digest [--dry-run] [--retention-days <1..3650>] [--refinement-handoff-retention-days <1..3650>] [--refinement-done-retention-days <1..3650>] [--operational-retention-days <1..3650>] [--pressure-age-days <1..3650>]
example: octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact
note: expires ACTIVE memories, purges old SUPERSEDED rows, expired locks, terminal refinements, and terminal standalone runs; reports signal/reference pressure but never prunes signals
schema: octocode-awareness schema json-schema digest --compact`,
  'pre-flight-intent': `usage: octocode-awareness lock acquire --agent-id <id> --target-file <p>... [--run-id <claimed-run>] [--workspace <p>] [--artifact <a>] [--rationale <t>] [--test-plan <t>] [--ttl-minutes <n>] [--wait-seconds <n>]
example: octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact
note: lock acquire is exclusive protection for sensitive work; ordinary work uses work start
note: --run-id attaches exclusive protection to a claimed task run
note: export OCTOCODE_AGENT_ID for CLI+hooks; --strict-agent-id / OCTOCODE_STRICT_AGENT_ID=1 hard-fails when missing
schema: octocode-awareness schema json-schema lock_acquire --compact`,
  'agent-signal': `usage: octocode-awareness signal publish|list|reply|ack|resolve --agent-id <id> [--to-agent <id>]... [--signal-id <id>]... [--thread-id <id>] [--kind <k>] [--subject <t>] [--body <t>] [--file <p>]...
examples:
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --format hook --compact
  octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --file src/file.ts --workspace "$PWD" --compact
  octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact
list options: [--limit <n>] [--all|--unread-only] [--mark-read] [--include-bodies] [--format json|hook]
note: --format hook returns the notify briefing shape used by host hooks (list only)
schema: octocode-awareness schema json-schema agent_signal --compact`,
  'verify': `usage: octocode-awareness verify mark (--run-id <id>... | --all-pending) --agent-id <id> [--status SUCCESS|FAILED] [--message <t>] [--workspace <p>] [--artifact <a>]
example: octocode-awareness verify mark --agent-id agent --run-id run_123 --message "yarn test passed" --compact
note: prefer explicit --run-id; scope deliberate --all-pending use with --workspace
schema: octocode-awareness schema json-schema verify --compact`,
  'reflect': `usage: octocode-awareness reflect record --agent-id <id> --task <text> --outcome worked|partial|failed [--worked <t>] [--didnt-work <t>] [--judgment-note <t>] [--lesson <t>] [--fix-repo <t>] [--fix-harness <t>] [--fix-instructions <t>] [--fix-file <p>]... [--failure-signature <s>] [--eval-failure-json <json>]... [--duo] [--allow-similar] [--importance <1..10>] [--workspace <p>] [--artifact <a>] [--repo <r>] [--ref <r>]
example: octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep CLI nouns canonical" --compact
note: --outcome must be worked|partial|failed; unknown values hard-error
note: --fix-repo → repo-code refinement; --fix-harness → skill/tooling; --fix-instructions → feedback to the human instruction author (see reflect developer-review); refinement --quality values are good|bad|handoff|instructions
schema: octocode-awareness schema json-schema reflect --compact`,
  'developer-review': `usage: octocode-awareness reflect developer-review [--workspace <repo>] [--state open|ongoing|done]... [--format json|markdown] [--limit <n>]
example: octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact
note: reads agent feedback on the instructions themselves (from reflect record --fix-instructions); use --format markdown for an explicit export`,
  'query': `usage: octocode-awareness query <all|repo-profile|memories|gotchas|lessons|plans|tasks|runs|locks|agents|signals|refinements|files|activity|workboard|developer-review> [--query <text>] [--limit <1..500>] [--workspace <repo>] [--artifact <a>] [--repo <r>] [--ref <r>] [--agent-id <id>] [--state <s>]... [--label <l>]... [--file <p>] [--since <iso>] [--include-bodies] [--format json|table|csv|markdown|html] [--out <path>]
examples:
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
note: files/memories expose missing file references as file_exists, missing_file, missing_references, and stale_file_refs workboard reasons
schema: octocode-awareness schema json-schema query --compact`,
  'attend': `usage: octocode-awareness attend [--workspace <repo>] [--query <text>] [--agent-id <id>] [--file <p>]... [--limit <n>] [--include-bodies] [--explain-organ]
example: octocode-awareness attend --query "current task" --workspace "$PWD" --agent-id "$OCTOCODE_AGENT_ID" --compact
note: pass --agent-id (or OCTOCODE_AGENT_ID) so next routes owned Verify/Claimed before generic evidence
schema: octocode-awareness schema json-schema attend --compact`,
  'repo-inject': `usage: octocode-awareness wiki sync [--workspace <repo>] [--out .octocode] [--mode local|share] [--no-check] [--no-include-view] [--prune-orphans]
example: octocode-awareness wiki sync --workspace "$PWD" --out .octocode --mode local --compact
note: review orphan_candidates before rerunning with --prune-orphans
schema: octocode-awareness schema json-schema wiki_sync --compact`,
  'docs-catalog': `usage: octocode-awareness docs list|show [name] [--full]
examples:
  octocode-awareness docs list --compact
  octocode-awareness docs show agent-cheatsheet
  octocode-awareness docs show agent-cheatsheet --compact  # JSON only
schema: octocode-awareness schema json-schema docs_catalog --compact`,
  'plan-command': `usage: octocode-awareness plan create|list|show|join|doc|status [options]
create: --name <text> --objective <text> --lead-agent-id <id> --workspace <repo> [--artifact <name>]
list: [--workspace <repo>] [--status <status>] [--limit <1-200>] [--full]
show/join/doc/status: --plan-id <id>; join also --agent-id <id>; doc uses --agent-id <member> --path docs/NOTE.md --title <text>; status uses --agent-id <lead> --status DRAFT|ACTIVE|PAUSED|COMPLETED|CANCELLED
example: octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact
schema: octocode-awareness schema json-schema plan --compact`,
  'task-command': `usage: octocode-awareness task create|list|ready|show|claim|heartbeat|submit|release|depend [options]
create: --plan-id <id> --title <text> --reasoning <text> --acceptance <text> --path <workspace-relative>... --agent-id <id> [--depends-on <task-id>]... [--priority <-1000..1000>] [--lease-minutes <1..60>] [--test-plan <text>]
list/ready: [--plan-id <id>] [--workspace <repo>] [--status <s>] [--limit <1-200>] [--full]
show: --task-id <id>
claim: --task-id <id> --agent-id <id>; or --next --plan-id <id> --agent-id <id>. Returns run_id for lock/submit/verify; exit 2 only when another live claimant owns it.
heartbeat/submit/release: --task-id <id> --run-id <id> --agent-id <id>; submit optionally --message <text>; release optionally --blocked-reason <text>
depend: --task-id <id> --depends-on <task-id>...
example: octocode-awareness task ready --plan-id plan_123 --compact
schema: octocode-awareness schema json-schema task --compact`,
  'work-command': `usage: octocode-awareness work start|touch|end|list|show [options]
start new WORK: --file <path>... --agent-id <id> [--workspace <repo>] --rationale <text> --test-plan <text> [--exclusive]
attach task run: --run-id <claimed-task-run> --file <path>... --agent-id <id> [--exclusive]
touch/end: --run-id <id> --agent-id <id> [--file <path>]...
list: [--workspace <repo>] [--agent-id <id>] [--run-id <id>] [--all] [--limit <1-200>] [--full]
show: --workspace <repo> --file <path> [--all] [--limit <1-200>] [--full]
example: octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact
schema: octocode-awareness schema json-schema work --compact`,
  'hook-run': `usage: octocode-awareness hook run <pre-edit|post-edit|stop-verify|notify-deliver|session-compact|session-end> < hook-payload.json
payload: host JSON on stdin; common fields are cwd/workspace, session_id, tool_name, and tool_input/path
store: hook run intentionally rejects --db; set OCTOCODE_MEMORY_HOME to select the hook database`,
  'hooks-install': hooksInstallUsage(),
  'schema': `usage: octocode-awareness schema commands|list|path <name>|command <noun> [action]|json-schema <name>|example <name>|validate <name> <json-file|->
examples:
  octocode-awareness schema commands --compact
  octocode-awareness schema command memory recall --compact
  octocode-awareness schema json-schema query --compact`,
  'init': `usage: octocode-awareness maintenance init [--db <path>]
example: octocode-awareness maintenance init --db .octocode/awareness.sqlite3 --compact`,
  'self-test': `usage: octocode-awareness maintenance self-test
example: octocode-awareness maintenance self-test --compact`,
};

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
      `usage: octocode-awareness ${noun}${actionList ? ` ${actionList}` : ''} [options]`,
      `details: octocode-awareness ${firstRoute} --help`,
      `map: octocode-awareness schema command ${firstRoute} --compact`,
    ].join('\n');
  }
  if (!command) return options.compact ? HELP_COMPACT : HELP;
  const normalized = command.replace(/_/g, '-');
  const flags = KNOWN_FLAGS[normalized];
  if (!flags) return HELP;
  const schema = COMMAND_TO_SCHEMA[normalized] ?? null;
  const display = options.routeKey ?? COMMAND_DISPLAY[normalized] ?? normalized;
  const example = (options.routeKey ? ROUTE_EXAMPLE[options.routeKey] : undefined) ?? COMMAND_EXAMPLE[normalized];
  if (options.compact) {
    return [
      `usage: octocode-awareness ${display} [options]`,
      schema ? `schema: ${schema}` : 'schema: none',
      `example: ${example ?? `octocode-awareness ${display}`}`,
    ].join('\n').trimEnd();
  }
  if (COMMAND_HELP[normalized]) return COMMAND_HELP[normalized]!;
  return [
    `usage: octocode-awareness ${display} [options]`,
    `flags: ${flags.map(hyphenFlag).join(' ')}`,
    schema ? `schema: octocode-awareness schema json-schema ${schema} --compact` : 'schema: none',
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
