import type { DatabaseSync } from 'node:sqlite';
import { connectDb, resolveDbPath } from '../src/db.js';
import { mineWeakness } from '../src/memory.js';
import { pruneStale, notifyGet, sessionCapture, waitForLock, digest, exportMemoryDoc } from '../src/maintenance.js';
import { runHooksInstall } from '../src/hooks-install.js';
import { runHookCommand } from './hook-runner.js';
import { commandFromHelpArgv, helpFor } from './cli-help.js';
import { EmitOptions, KNOWN_FLAGS, UNKNOWN_COMMAND, die, emit, extractGlobalDb, packageSkillScriptPath, parseBoundedSeconds, resolveAgentId, selectCommand, setActiveCommand, validateFlagValues, validateFlags } from './cli-routing.js';
import { MAX_CLI_RETRY_INTERVAL_SECONDS, MAX_CLI_WAIT_SECONDS, parseArgs } from './cli-model.js';
import { COMMAND_DISPLAY, COMMAND_EXAMPLE, COMMAND_TO_SCHEMA, HELP, HELP_COMPACT } from './cli-help-data.js';
import { cmdAgentRegistry, cmdAgentSignal, cmdInit, cmdNotifyPrune, cmdSelfTest, cmdStatus } from './cli-admin.js';
import { cmdGetMemory, cmdRefineGet, cmdRefineSet, cmdReflect, cmdTellMemory } from './cli-memory.js';
import { cmdAuditUnverified, cmdPreFlightIntent, cmdReleaseFileLock, cmdVerify, cmdWork } from './cli-work.js';
import { cmdExportHarness, cmdForget, cmdMemoryLifecycle, cmdPlan, cmdRefineDelete, cmdTask } from './cli-plans.js';
import { cmdAttend, cmdDeveloperReview, cmdDocStaleness, cmdDocsCatalog, cmdQuery, cmdRepoInject } from './cli-repo.js';

// ─── Entry point ──────────────────────────────────────────────────────────────

export const rawArgv = process.argv.slice(2);

if (rawArgv.length === 0 || rawArgv.includes('--help') || rawArgv.includes('-h')) {
  const compactHelp = rawArgv.includes('--compact') || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
  const helpTarget = commandFromHelpArgv(rawArgv);
  process.stdout.write(helpFor(helpTarget.command, { compact: compactHelp, routeKey: helpTarget.routeKey }) + '\n');
  process.exit(0);
}

export const { dbPath: globalDb, filtered: filteredArgv } = extractGlobalDb(rawArgv);
export const { command, rest } = selectCommand(filteredArgv);
setActiveCommand(command ?? '');
export const args = parseArgs(rest ?? []);
if (globalDb) args['db'] = globalDb;

// Unknown flags are hard errors — a silently ignored flag reads as "it worked".
if (command && KNOWN_FLAGS[command]) {
  const unknown = validateFlags(command, args);
  if (unknown.length > 0) {
    const compactError = args['compact'] === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
    const payload = {
      ok: false,
      command: COMMAND_DISPLAY[command] ?? command,
      schema: COMMAND_TO_SCHEMA[command] ?? null,
      error: `unknown flag(s): ${unknown.map((f) => `--${f.replace(/_/g, '-')}`).join(', ')}`,
      known_flags: KNOWN_FLAGS[command]!.map((f) => `--${f.replace(/_/g, '-')}`),
      hint: `Run "octocode-awareness ${COMMAND_DISPLAY[command] ?? command} --help" for this command.`,
      example: COMMAND_EXAMPLE[command],
    };
    process.stdout.write(JSON.stringify(payload, null, compactError ? 0 : 2) + '\n');
    process.exit(1);
  }
}
if (command && command !== UNKNOWN_COMMAND) validateFlagValues(args);

export const dbPath = resolveDbPath(globalDb ?? null);
export const compact = args['compact'] === true || process.env['OCTOCODE_AWARENESS_COMPACT'] === '1';
export const opts: EmitOptions = { compact };

if (!command) {
  process.stdout.write((compact ? HELP_COMPACT : HELP) + '\n');
  process.exit(0);
}

if (command === UNKNOWN_COMMAND) {
  const requested = filteredArgv.slice(0, 2).join(' ') || filteredArgv[0] || '';
  const payload = {
    ok: false,
    error: `unknown command: ${requested}`,
    hint: 'Use canonical noun/verb commands only; run "octocode-awareness --help" for the command map.',
    examples: [
      'octocode-awareness memory recall --query "current task" --workspace "$PWD" --compact',
      'octocode-awareness lock acquire --agent-id agent --target-file src/file.ts --rationale "edit" --compact',
      'octocode-awareness signal list --agent-id agent --workspace "$PWD" --limit 3 --compact',
      'octocode-awareness query gotchas --workspace "$PWD" --format json --limit 20 --compact',
    ],
  };
  process.stdout.write(JSON.stringify(payload, null, compact ? 0 : 2) + '\n');
  process.exit(1);
}

if (command === 'self-test') {
  process.exit(cmdSelfTest(opts));
}

if (command === 'schema') {
  const { runSchemaCli } = await import('../src/schema/cli.js');
  process.exit(await runSchemaCli(rest));
}

if (command === 'hook-run') {
  // Hooks always write to the canonical store; a `--db` here was silently
  // ignored (edits would land in the real DB regardless), which is a footgun.
  // Fail loudly instead of misleading the caller.
  if (globalDb) die('hook run ignores --db: hooks always use the canonical store. Remove --db, or set OCTOCODE_MEMORY_HOME to relocate the store.');
  process.exit(await runHookCommand(String(args._[0] ?? 'help')));
}

if (command === 'hooks-install') {
  const result = runHooksInstall(rest, { hookDir: packageSkillScriptPath('hooks'), dbPath });
  if (result.text !== undefined) process.stdout.write(result.text);
  else if (result.payload) emit(result.payload, result.exitCode, opts);
  process.exit(result.exitCode);
}

export let db: DatabaseSync;
try {
  db = connectDb(dbPath);
} catch (err) {
  process.stderr.write(`octocode-awareness: failed to connect DB at ${dbPath}: ${String(err)}\n`);
  process.exit(1);
}

export let exitCode = 0;
try {
  switch (command) {
    case 'tell-memory':    exitCode = cmdTellMemory(db, args, dbPath, opts); break;
    case 'get-memory':     exitCode = cmdGetMemory(db, args, dbPath, opts); break;
    case 'reflect':        exitCode = cmdReflect(db, args, dbPath, opts); break;
    case 'refine-set':     exitCode = cmdRefineSet(db, args, dbPath, opts); break;
    case 'refine-get':     exitCode = cmdRefineGet(db, args, dbPath, opts); break;
    case 'pre-flight-intent': exitCode = cmdPreFlightIntent(db, args, dbPath, opts); break;
    case 'release-file-lock': exitCode = cmdReleaseFileLock(db, args, dbPath, opts); break;
    case 'plan-command':   exitCode = cmdPlan(db, args, dbPath, opts); break;
    case 'task-command':   exitCode = cmdTask(db, args, dbPath, opts); break;
    case 'status':         exitCode = cmdStatus(db, dbPath, args, opts); break;
    case 'init':           exitCode = cmdInit(db, dbPath, opts); break;
    case 'prune-stale-locks': exitCode = emit({ db_path: dbPath, ...pruneStale(db, args) }, 0, opts); break;
    case 'audit-unverified':  exitCode = cmdAuditUnverified(db, args, dbPath, opts); break;
    case 'verify':             exitCode = cmdVerify(db, args, dbPath, opts); break;
    case 'session-capture': exitCode = emit({
      db_path: dbPath,
      ...sessionCapture(db, {
        agent_id: resolveAgentId(args),
        workspace: args['workspace'],
        artifact: args['artifact'],
        repo: args['repo'],
        ref: args['ref'],
        reason: args['reason'],
        cwd: args['cwd'],
      }),
    }, 0, opts); break;
    case 'mine-weakness': {
      const mwParams = {
        agentId:       args['agent_id'] as string | undefined,
        workspacePath: args['workspace'] as string | undefined,
        artifact:      args['artifact'] as string | undefined,
        minCount:      args['min_count'] ? Number(args['min_count']) : undefined,
        limit:         args['limit']     ? Number(args['limit'])     : undefined,
        cwd:           args['cwd']       as string | undefined,
      };
      exitCode = emit({ db_path: dbPath, ...mineWeakness(db, mwParams) }, 0, opts);
      break;
    }
    case 'doc-staleness': exitCode = cmdDocStaleness(db, args, dbPath, opts); break;
    case 'docs-catalog': exitCode = cmdDocsCatalog(db, args, dbPath, opts); break;
    case 'digest': {
      const retDays = args['retention_days'] ? Number(args['retention_days']) : undefined;
      const handoffDays = args['refinement_handoff_retention_days'] ? Number(args['refinement_handoff_retention_days']) : undefined;
      const doneDays = args['refinement_done_retention_days'] ? Number(args['refinement_done_retention_days']) : undefined;
      const operationalDays = args['operational_retention_days'] ? Number(args['operational_retention_days']) : undefined;
      const pressureAgeDays = args['pressure_age_days'] ? Number(args['pressure_age_days']) : 1;
      const isDryRun = Boolean(args['dry_run'] ?? args['dry-run']);
      const digestResult = digest(db, {
        ...(retDays !== undefined ? { retention_days: retDays } : {}),
        ...(handoffDays !== undefined ? { refinement_handoff_retention_days: handoffDays } : {}),
        ...(doneDays !== undefined ? { refinement_done_retention_days: doneDays } : {}),
        ...(operationalDays !== undefined ? { operational_retention_days: operationalDays } : {}),
        pressure_age_days: pressureAgeDays,
        ...(args['workspace'] ? { workspace: String(args['workspace']) } : {}),
        ...(args['artifact'] ? { artifact: String(args['artifact']) } : {}),
        ...(isDryRun ? { dry_run: true } : {}),
      });
      const payload: Record<string, unknown> = { db_path: dbPath, ...digestResult };
      if (!isDryRun && (args['export_doc'] ?? args['export-doc'])) {
        try {
          const wsPath = (args['workspace'] as string | undefined) ?? process.cwd();
          const artifact = args['artifact'] as string | undefined;
          const { mkdirSync, writeFileSync } = await import('node:fs');
          const { join } = await import('node:path');
          const docDir = join(wsPath, '.octocode', 'memory-reports');
          mkdirSync(docDir, { recursive: true });
          const dateStr = new Date().toISOString().slice(0, 16).replace('T', '-').replace(':', '');
          const docPath = (typeof (args['export_doc'] ?? args['export-doc']) === 'string'
            ? args['export_doc'] ?? args['export-doc']
            : join(docDir, `memory-report-${dateStr}.md`)) as string;
          writeFileSync(docPath, exportMemoryDoc(db, { workspace_path: wsPath, artifact }), 'utf8');
          payload['doc_path'] = docPath;
        } catch (err) {
          payload['doc_warning'] = `Could not write doc: ${(err as Error).message}`;
        }
      }
      exitCode = emit(payload, 0, opts);
      break;
    }
    case 'wait-for-lock': {
      const rawWaitTarget = args['target_file'] ?? args['file'];
      const waitTargets = Array.isArray(rawWaitTarget) ? rawWaitTarget : rawWaitTarget ? [String(rawWaitTarget)] : [];
      const waitSecs = parseBoundedSeconds(args, 'wait_seconds', 0, MAX_CLI_WAIT_SECONDS);
      const retrySecs = parseBoundedSeconds(args, 'retry_interval', 1, MAX_CLI_RETRY_INTERVAL_SECONDS);
      const waitResult = waitForLock(db, {
        agent_id: resolveAgentId(args),
        target_files: waitTargets,
        workspace: args['workspace'],
        artifact: args['artifact'],
        wait_ms: waitSecs != null ? waitSecs * 1000 : undefined,
        retry_interval_ms: retrySecs != null ? retrySecs * 1000 : undefined,
      });
      exitCode = emit({ db_path: dbPath, ...waitResult }, waitResult.lock_free ? 0 : 2, opts);
      break;
    }
    case 'work-command':    exitCode = cmdWork(db, args, dbPath, opts); break;
    case 'forget':          exitCode = cmdForget(db, args, dbPath, opts); break;
    case 'memory-archive':  exitCode = cmdMemoryLifecycle(db, args, dbPath, opts, 'archive'); break;
    case 'memory-restore':  exitCode = cmdMemoryLifecycle(db, args, dbPath, opts, 'restore'); break;
    case 'refine-delete':   exitCode = cmdRefineDelete(db, args, dbPath, opts); break;
    case 'export-harness':  exitCode = cmdExportHarness(db, args, dbPath, opts); break;
    case 'developer-review': exitCode = cmdDeveloperReview(db, args, dbPath, opts); break;
    case 'query':           exitCode = cmdQuery(db, args, dbPath, opts); break;
    case 'attend':          exitCode = cmdAttend(db, args, dbPath, opts); break;
    case 'repo-inject':     exitCode = cmdRepoInject(db, args, dbPath, opts); break;
    case 'agent-registry':  exitCode = cmdAgentRegistry(db, args, dbPath, opts); break;
    case 'agent-signal': {
      const signalFormat = String(args['format'] ?? 'json');
      if (args['action'] === 'list' && signalFormat === 'hook') {
        const signalBriefing = notifyGet(db, {
          workspace: args['workspace'] as string | undefined,
          artifact: args['artifact'] as string | undefined,
          format: signalFormat,
          agent_id: args['agent_id'] as string | undefined,
        }) as unknown as Record<string, unknown>;
        exitCode = signalBriefing['additionalContext']
          ? emit({ additionalContext: signalBriefing['additionalContext'] }, 0, opts)
          : emit({ db_path: dbPath, ...signalBriefing }, 0, opts);
      } else {
        exitCode = cmdAgentSignal(db, args, dbPath, opts);
      }
      break;
    }
    case 'notify-prune':    exitCode = cmdNotifyPrune(db, args, dbPath, opts); break;
    default:
      exitCode = emit({ error: `unknown command: ${command}. Run --help for usage.` }, 1, opts);
  }
} catch (err) {
  // Domain errors thrown from src/* land here; emit() attaches the same
  // {command,schema,example} context that flag-parse errors get from die().
  exitCode = emit({
    error: err instanceof Error ? err.message : String(err),
  }, 1, opts);
}

process.exit(exitCode);
