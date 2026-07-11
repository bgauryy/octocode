import { BUNDLED_SKILLS, BUNDLED_SKILLS_BLOCK, BUNDLED_SKILLS_DIR } from './cli-model.js';

// ─── Help text ────────────────────────────────────────────────────────────────

export const HELP = `usage: octocode-awareness <command> [options]
common: --db <path> --compact (except hook run; hooks use OCTOCODE_MEMORY_HOME)
agent map: octocode-awareness schema commands --compact
schema: octocode-awareness schema commands|list|path <name>|json-schema <name>|example <name>|validate <name> <json-file|->

<AGENT_INSTRUCTIONS>
You are reading the octocode-awareness CLI. Use --compact for operational JSON; read docs show as raw Markdown.

BUNDLED SKILLS (${BUNDLED_SKILLS.length}):
${BUNDLED_SKILLS_BLOCK}

  The octocode-awareness skill owns operating policy for memory, locks, planning,
  coordination, verification, repo context, and hooks. Focused help/schema owns flags and payloads.
  octocode-skills is required only to discover, install, or review skills; every other skill
  above is optional — install only the ones the current task needs.

  Install any bundled skill from its path above using your agent platform's skill mechanism, e.g.:
    npx octocode skill --add --path "<path from above>" --platform <claude|codex|cursor|common>
  Do not use a skill installer's registry/name lookup for these; always install from the bundled path.

FIRST COMMANDS after install:
  octocode-awareness attend --workspace "$PWD" --query "<task>" --agent-id "$OCTOCODE_AGENT_ID" --compact
  octocode-awareness workspace status --compact                    # DB health
  octocode-awareness schema commands --compact                     # full command map
  octocode-awareness docs show agent-cheatsheet                    # operating loop

COMMAND SURFACES:
  start:     attend, workspace status, plan list, task ready, memory recall, signal list, query <view>
  planning:  plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend
  edit:      work start|touch|end|list|show; lock acquire, lock wait, lock release, lock prune; verify mark, verify audit
  messages:  signal publish, signal list, signal reply, signal ack, signal resolve, signal prune, agent register, agent list
  learning:  memory record, memory archive, memory restore, memory forget, refinement set, refinement get, refinement delete, reflect record, reflect mine-weakness, reflect export-harness, reflect developer-review, docs list, docs show, docs staleness
  repo:      query files|workboard|all|developer-review [--format json|table|csv|markdown|html], repo inject
  hooks:     hook run <pre-edit|post-edit|stop-verify|notify-deliver|session-compact|session-end>, hooks install|check|remove --host claude|codex|cursor
  utility:   session capture, maintenance init, maintenance self-test, maintenance digest
</AGENT_INSTRUCTIONS>

supported agents: Codex, Claude Code, Cursor, Pi, and custom library/CLI hosts
activation: AGENTS.md = trigger/router; Agent Skill = operating policy; CLI/SQLite = canonical live state; hooks/Pi bridge = deterministic lifecycle automation

examples:
  octocode-awareness workspace status --workspace "$PWD" --compact
  octocode-awareness attend --workspace "$PWD" --query "current task" --compact
  octocode-awareness task ready --plan-id plan_123 --compact
  octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact
  octocode-awareness docs list --compact
  octocode-awareness docs show agent-cheatsheet
  octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact
  octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact
  octocode-awareness schema commands --compact
  octocode-awareness query files --workspace "$PWD" --format table --limit 50
  octocode-awareness query all --workspace "$PWD" --format html --out .octocode/awareness/index.html
  octocode-awareness repo inject --workspace "$PWD" --mode local --compact

Run "octocode-awareness <command> --help" for command flags.
Exit codes: 0 ok; 1 validation/unknown flag/verify debt (verify audit); 2 live task/lock conflict, lock wait timeout, or hooks check --strict.`;

export const HELP_COMPACT = `octocode-awareness: canonical noun/verb CLI; AGENTS routes → skill decides → CLI/SQLite acts → hooks automate edges. Use --compact for JSON.
bundled-skills(${BUNDLED_SKILLS.length}): ${BUNDLED_SKILLS_DIR} — octocode-awareness+octocode-skills required, others optional (see --help)
start: attend; workspace status; plan create|list|show|join|doc|status; task create|list|ready|show|claim|heartbeat|submit|release|depend; memory recall; signal list; docs list
edit: work start|touch|end|list|show; lock acquire|wait|release|prune; verify audit|mark
msg: signal publish|list|reply|ack|resolve|prune; agent register|list
learn: memory record|archive|restore|forget; refinement set|get|delete; reflect record|mine-weakness|export-harness|developer-review; maintenance digest
repo: query files|workboard|all|developer-review --format json|table|csv|markdown|html; repo inject
inspect: schema commands --compact; docs list|show; <command> --help; exits 0 ok / 1 validation|verify debt / 2 live claim|lock|wait|hooks --strict`;

export const COMMAND_TO_SCHEMA: Record<string, string> = {
  'tell-memory': 'tell_memory',
  'get-memory': 'get_memory',
  'memory-archive': 'memory_lifecycle',
  'memory-restore': 'memory_lifecycle',
  'pre-flight-intent': 'pre_flight_intent',
  'wait-for-lock': 'wait_for_lock',
  'prune-stale-locks': 'prune_stale_locks',
  'release-file-lock': 'release_file_lock',
  'audit-unverified': 'audit_unverified',
  'verify': 'verify',
  'forget': 'forget_memory',
  'refine-set': 'refinement',
  'refine-get': 'refine_query',
  'refine-delete': 'refine_delete',
  'agent-registry': 'agent_registry',
  'agent-signal': 'agent_signal',
  'notify-prune': 'signal_prune',
  'status': 'workspace_status',
  'attend': 'attend',
  'export-harness': 'export_harness',
  'query': 'query',
  'repo-inject': 'repo_inject',
  'session-capture': 'session_capture',
  'mine-weakness': 'mine_weakness',
  'doc-staleness': 'doc_staleness',
  'docs-catalog': 'docs_catalog',
  'digest': 'digest',
  'reflect': 'reflect',
  'plan-command': 'plan',
  'task-command': 'task',
  'work-command': 'work',
};

export const COMMAND_DISPLAY: Record<string, string> = {
  'tell-memory': 'memory record',
  'get-memory': 'memory recall',
  'memory-archive': 'memory archive',
  'memory-restore': 'memory restore',
  'forget': 'memory forget',
  'pre-flight-intent': 'lock acquire',
  'wait-for-lock': 'lock wait',
  'prune-stale-locks': 'lock prune',
  'release-file-lock': 'lock release',
  'audit-unverified': 'verify audit',
  'verify': 'verify mark',
  'refine-set': 'refinement set',
  'refine-get': 'refinement get',
  'refine-delete': 'refinement delete',
  'agent-registry': 'agent register|list',
  'agent-signal': 'signal publish|list|reply|ack|resolve',
  'notify-prune': 'signal prune',
  'status': 'workspace status',
  'attend': 'attend',
  'export-harness': 'reflect export-harness',
  'developer-review': 'reflect developer-review',
  'query': 'query',
  'repo-inject': 'repo inject',
  'session-capture': 'session capture',
  'mine-weakness': 'reflect mine-weakness',
  'doc-staleness': 'docs staleness',
  'docs-catalog': 'docs list|show',
  'digest': 'maintenance digest',
  'init': 'maintenance init',
  'self-test': 'maintenance self-test',
  'reflect': 'reflect record',
  'plan-command': 'plan create|list|show|join|doc|status',
  'task-command': 'task create|list|ready|show|claim|heartbeat|submit|release|depend',
  'work-command': 'work start|touch|end|list|show',
  'hook-run': 'hook run',
  'hooks-install': 'hooks install|check|remove',
  'schema': 'schema',
};

export const COMMAND_EXAMPLE: Record<string, string> = {
  'tell-memory': 'octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact',
  'get-memory': 'octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact',
  'memory-archive': 'octocode-awareness memory archive --memory-id mem_123 --dry-run --compact',
  'memory-restore': 'octocode-awareness memory restore --memory-id mem_123 --dry-run --compact',
  'forget': 'octocode-awareness memory forget --memory-id mem_123 --dry-run --compact',
  'pre-flight-intent': 'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact',
  'wait-for-lock': 'octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact',
  'prune-stale-locks': 'octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact',
  'release-file-lock': 'octocode-awareness lock release --agent-id agent --run-id run_123 --status PENDING --compact',
  'audit-unverified': 'octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact',
  'verify': 'octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact',
  'refine-set': 'octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact',
  'refine-get': 'octocode-awareness refinement get --workspace "$PWD" --state open --compact',
  'refine-delete': 'octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact',
  'agent-registry': 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  'agent-signal': 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  'notify-prune': 'octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact',
  'status': 'octocode-awareness workspace status --workspace "$PWD" --compact',
  'attend': 'octocode-awareness attend --query "current task" --workspace "$PWD" --compact',
  'export-harness': 'octocode-awareness reflect export-harness --workspace "$PWD" --compact',
  'developer-review': 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'query': 'octocode-awareness query workboard --workspace "$PWD" --format json --limit 10 --compact',
  'repo-inject': 'octocode-awareness repo inject --workspace "$PWD" --out .octocode --mode local --compact',
  'session-capture': 'octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact',
  'mine-weakness': 'octocode-awareness reflect mine-weakness --workspace "$PWD" --compact',
  'doc-staleness': 'octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact',
  'docs-catalog': 'octocode-awareness docs list --compact',
  'digest': 'octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact',
  'init': 'octocode-awareness maintenance init --compact',
  'self-test': 'octocode-awareness maintenance self-test --compact',
  'reflect': 'octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep commands canonical" --compact',
  'plan-command': 'octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact',
  'task-command': 'octocode-awareness task ready --plan-id plan_123 --compact',
  'work-command': 'octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact',
  'hook-run': 'octocode-awareness hook run pre-edit < hook-payload.json',
  'hooks-install': 'octocode-awareness hooks install --host codex --dry-run --compact',
  'schema': 'octocode-awareness schema commands --compact',
};

export const ROUTE_EXAMPLE: Record<string, string> = {
  'signal publish': 'octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact',
  'signal list': 'octocode-awareness signal list --agent-id agent --workspace "$PWD" --compact',
  'signal reply': 'octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact',
  'signal ack': 'octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact',
  'signal resolve': 'octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact',
  'agent register': 'octocode-awareness agent register --agent-id agent --agent-name "Codex" --workspace "$PWD" --compact',
  'agent list': 'octocode-awareness agent list --workspace "$PWD" --compact',
  'reflect developer-review': 'octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'docs list': 'octocode-awareness docs list --compact',
  'docs show': 'octocode-awareness docs show agent-cheatsheet',
  'hooks install': 'octocode-awareness hooks install --host codex --dry-run --compact',
  'hooks check': 'octocode-awareness hooks check --host codex --strict --compact',
  'hooks remove': 'octocode-awareness hooks remove --host codex --dry-run --compact',
  'schema commands': 'octocode-awareness schema commands --compact',
  'schema list': 'octocode-awareness schema list --compact',
  'schema json-schema': 'octocode-awareness schema json-schema get_memory --compact',
  'schema example': 'octocode-awareness schema example get_memory --compact',
  'schema validate': 'octocode-awareness schema validate get_memory payload.json --compact',
};
