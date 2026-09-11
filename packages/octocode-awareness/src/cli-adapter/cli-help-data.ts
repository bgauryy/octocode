import { BUNDLED_SKILLS, BUNDLED_SKILLS_DIR } from './cli-model.js';
import { HISTORY_ROUTE_DESCRIPTORS } from '../schema/definitions-history.js';

// ─── Help text ────────────────────────────────────────────────────────────────

export const HELP = `  🐙 Octocode Awareness

  ONE SURFACE · FIVE CONCEPTS · NINETEEN OPERATIONS
    context  orient
    work     create · list · show · claim · update · depend · protect · verify
    message  list · send · reply · resolve
    memory   recall · record
    history  status · timeline · read · restore

  ROUTINE  context orient → call the selected <concept> <operation> directly
  EXAMPLE  work create --kind standalone --file src/a.ts --rationale "edit" --test-plan "yarn test"
  SCHEMA   schema command <concept> <operation> --compact
  MAP      schema commands --compact

  OPERATOR  schema commands --all --compact
    Configuration, hooks, migration, retention, maintenance, and diagnostics stay out of routine context.

  RUNNER  npx @octocodeai/octocode-awareness <concept> <operation> [options]
  CONTEXT  --workspace <repo> · --agent-id <id> · --db-scope repo|global · --db <path>
  OUTPUT   --compact lean JSON; partial results include executable next calls
  EXIT    0 ok · 1 input/verification debt · 2 conflict/wait/strict hook health
  SKILL   ${BUNDLED_SKILLS.length} bundled at ${BUNDLED_SKILLS_DIR}; install is an explicit operator action`;

export const HELP_COMPACT = `octocode-awareness: one direct CLI surface; call context orient once, then <concept> <operation>.
concepts: context=orient; work=create|list|show|claim|update|depend|protect|verify; message=list|send|reply|resolve; memory=recall|record; history=status|timeline|read|restore
schema: schema commands --compact; exact: schema command <concept> <operation> --compact; operator/recovery: schema commands --all --compact
context: --workspace <repo> --agent-id <id> --db-scope repo|global --db <path>; partial results retain executable next calls
exits: 0 ok / 1 validation|verification debt / 2 conflict|wait|strict hook health`;

export const COMMAND_TO_SCHEMA: Record<string, string> = {
  ...Object.fromEntries(HISTORY_ROUTE_DESCRIPTORS.map(route => [route.schema, route.schema])),
  'tell-memory': 'memory_record',
  'get-memory': 'memory_recall',
  'memory-archive': 'memory_lifecycle',
  'memory-restore': 'memory_lifecycle',
  'pre-flight-intent': 'lock_acquire',
  'wait-for-lock': 'lock_wait',
  'prune-stale-locks': 'lock_prune',
  'release-file-lock': 'lock_release',
  'audit-unverified': 'verify_audit',
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
  'session-capture': 'session_capture',
  'mine-weakness': 'mine_weakness',
  'doc-staleness': 'doc_staleness',
  'docs-catalog': 'docs_catalog',
  'skill-install': 'skill_install',
  'skill-list': 'skill_list',
  'skill-check': 'skill_check',
  'skill-remove': 'skill_remove',
  'digest': 'digest',
  'reflect': 'reflect',
  'plan-command': 'plan',
  'task-command': 'task',
  'work-command': 'work',
  'awareness-config': 'awareness_config',
};

export const COMMAND_DISPLAY: Record<string, string> = {
  ...Object.fromEntries(HISTORY_ROUTE_DESCRIPTORS.map(route => [route.schema, route.command])),
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
  'session-capture': 'session capture',
  'mine-weakness': 'reflect mine-weakness',
  'doc-staleness': 'docs staleness',
  'docs-catalog': 'docs list|show',
  'skill-install': 'skill install',
  'skill-list': 'skill list',
  'skill-check': 'skill check',
  'skill-remove': 'skill remove',
  'digest': 'maintenance digest',
  'init': 'maintenance init',
  'self-test': 'maintenance self-test',
  'reflect': 'reflect record',
  'plan-command': 'plan create|list|show|join|doc|status',
  'task-command': 'task create|list|ready|show|claim|heartbeat|submit|release|retry|depend',
  'work-command': 'work start|touch|end|list|show',
  'hook-run': 'hook run',
  'hooks-install': 'hooks install|check|remove',
  'schema': 'schema',
  'awareness-config': 'config show|init|validate',
};

export const COMMAND_EXAMPLE: Record<string, string> = {
  ...Object.fromEntries(HISTORY_ROUTE_DESCRIPTORS.map(route => [route.schema, route.example])),
  'tell-memory': 'npx @octocodeai/octocode-awareness memory record --agent-id agent --task-context "build failure" --observation "Run yarn build before tests" --importance 7 --label GOTCHA --workspace "$PWD" --compact',
  'get-memory': 'npx @octocodeai/octocode-awareness memory recall --query "current task" --workspace "$PWD" --smart --compact',
  'memory-archive': 'npx @octocodeai/octocode-awareness memory archive --memory-id mem_123 --dry-run --compact',
  'memory-restore': 'npx @octocodeai/octocode-awareness memory restore --memory-id mem_123 --dry-run --compact',
  'forget': 'npx @octocodeai/octocode-awareness memory forget --memory-id mem_123 --dry-run --compact',
  'pre-flight-intent': 'npx @octocodeai/octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit file" --test-plan "yarn test" --compact',
  'wait-for-lock': 'npx @octocodeai/octocode-awareness lock wait --agent-id agent --target-file src/file.ts --wait-seconds 60 --compact',
  'prune-stale-locks': 'npx @octocodeai/octocode-awareness lock prune --workspace "$PWD" --expired-only --dry-run --compact',
  'release-file-lock': 'npx @octocodeai/octocode-awareness lock release --agent-id agent --run-id run_123 --status PENDING --compact',
  'audit-unverified': 'npx @octocodeai/octocode-awareness verify audit --agent-id agent --workspace "$PWD" --compact',
  'verify': 'npx @octocodeai/octocode-awareness verify mark --agent-id agent --all-pending --message "yarn test passed" --workspace "$PWD" --compact # use --adopt-verification only for one prior-session --run-id after verifying',
  'refine-set': 'npx @octocodeai/octocode-awareness refinement set --agent-id agent --reasoning "handoff" --remember "next step" --workspace "$PWD" --compact',
  'refine-get': 'npx @octocodeai/octocode-awareness refinement get --workspace "$PWD" --state open --limit 3 --compact',
  'refine-delete': 'npx @octocodeai/octocode-awareness refinement delete --refinement-id ref_123 --dry-run --compact',
  'agent-registry': 'npx @octocodeai/octocode-awareness agent register --agent-id "$OCTOCODE_AGENT_ID" --agent-name "Parser reviewer" --agent-vendor "openai" --agent-host "codex" --workspace "$PWD" --compact',
  'agent-signal': 'npx @octocodeai/octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact',
  'notify-prune': 'npx @octocodeai/octocode-awareness signal prune --workspace "$PWD" --resolved --dry-run --compact',
  'status': 'npx @octocodeai/octocode-awareness workspace status --workspace "$PWD" --compact',
  'attend': 'npx @octocodeai/octocode-awareness attend --query "current task" --workspace "$PWD" --compact',
  'export-harness': 'npx @octocodeai/octocode-awareness reflect export-harness --workspace "$PWD" --compact',
  'developer-review': 'npx @octocodeai/octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'query': 'npx @octocodeai/octocode-awareness query workboard --workspace "$PWD" --format json --limit 1 --compact',
  'session-capture': 'npx @octocodeai/octocode-awareness session capture --agent-id agent --workspace "$PWD" --reason handoff --compact',
  'mine-weakness': 'npx @octocodeai/octocode-awareness reflect mine-weakness --workspace "$PWD" --compact',
  'doc-staleness': 'npx @octocodeai/octocode-awareness docs staleness --targets-json \'[{"docFile":"README.md","sourceDirs":["src"]}]\' --compact',
  'docs-catalog': 'npx @octocodeai/octocode-awareness docs list --compact',
  'skill-install': 'npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run',
  'skill-list': 'npx @octocodeai/octocode-awareness skill list --compact',
  'skill-check': 'npx @octocodeai/octocode-awareness skill check --compact',
  'skill-remove': 'npx @octocodeai/octocode-awareness skill remove --platform shared --project-dir "$PWD"',
  'digest': 'npx @octocodeai/octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact',
  'init': 'npx @octocodeai/octocode-awareness maintenance init --compact',
  'self-test': 'npx @octocodeai/octocode-awareness maintenance self-test --compact',
  'reflect': 'npx @octocodeai/octocode-awareness reflect record --agent-id agent --task "fix CLI" --outcome worked --lesson "Keep commands canonical" --compact',
  'plan-command': 'npx @octocodeai/octocode-awareness plan create --name "Release" --objective "Ship safely" --lead-agent-id agent --workspace "$PWD" --compact',
  'task-command': 'npx @octocodeai/octocode-awareness task ready --plan-id plan_123 --compact',
  'work-command': 'npx @octocodeai/octocode-awareness work start --agent-id agent --workspace "$PWD" --file src/a.ts --rationale "edit parser" --test-plan "yarn test" --compact',
  'hook-run': 'octocode-awareness hook run pre-edit < hook-payload.json',
  'hooks-install': 'npx @octocodeai/octocode-awareness hooks install --host codex --dry-run',
  'schema': 'npx @octocodeai/octocode-awareness schema commands --compact',
  'awareness-config': 'npx @octocodeai/octocode-awareness config show --compact',
};

export const ROUTE_EXAMPLE: Record<string, string> = {
  'maintenance init': 'npx @octocodeai/octocode-awareness maintenance init --compact',
  'config show': 'npx @octocodeai/octocode-awareness config show --compact',
  'config init': 'npx @octocodeai/octocode-awareness config init --hooks true --notifications true --verification-gate true --session-capture true --maintenance-reminders false --compact',
  'config validate': 'npx @octocodeai/octocode-awareness config validate --compact',
  'signal publish': 'npx @octocodeai/octocode-awareness signal publish --agent-id agent --kind blocker --subject "File locked" --workspace "$PWD" --compact',
  'signal list': 'npx @octocodeai/octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact',
  'signal reply': 'npx @octocodeai/octocode-awareness signal reply --agent-id agent --in-reply-to ntf_123 --subject "Re: File locked" --body "done" --compact',
  'signal ack': 'npx @octocodeai/octocode-awareness signal ack --agent-id agent --signal-id ntf_123 --compact',
  'signal resolve': 'npx @octocodeai/octocode-awareness signal resolve --agent-id agent --thread-id ntf_123 --compact',
  'agent register': 'npx @octocodeai/octocode-awareness agent register --agent-id "$OCTOCODE_AGENT_ID" --agent-name "Parser reviewer" --agent-vendor "openai" --agent-host "codex" --workspace "$PWD" --compact',
  'agent list': 'npx @octocodeai/octocode-awareness agent list --workspace "$PWD" --limit 5 --compact',
  'reflect developer-review': 'npx @octocodeai/octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact',
  'docs list': 'npx @octocodeai/octocode-awareness docs list --compact',
  'docs show': 'npx @octocodeai/octocode-awareness docs show agent-cheatsheet',
  'skill install': 'npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run',
  'skill list': 'npx @octocodeai/octocode-awareness skill list --compact',
  'skill check': 'npx @octocodeai/octocode-awareness skill check --compact',
  'skill remove': 'npx @octocodeai/octocode-awareness skill remove --platform shared --project-dir "$PWD"',
  'hooks install': 'npx @octocodeai/octocode-awareness hooks install --host codex --dry-run',
  'hooks check': 'npx @octocodeai/octocode-awareness hooks check --host codex --strict',
  'hooks remove': 'npx @octocodeai/octocode-awareness hooks remove --host codex --dry-run',
  'schema commands': 'npx @octocodeai/octocode-awareness schema commands --compact',
  'schema list': 'npx @octocodeai/octocode-awareness schema list --compact',
  'schema json-schema': 'npx @octocodeai/octocode-awareness schema json-schema memory_recall --compact',
  'schema example': 'npx @octocodeai/octocode-awareness schema example memory_recall --compact',
  'schema validate': 'npx @octocodeai/octocode-awareness schema validate memory_recall payload.json --compact',
};
