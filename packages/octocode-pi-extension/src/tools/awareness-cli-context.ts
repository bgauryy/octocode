import { resolveAwarenessHostBindings } from './awareness-context.js';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import { resolveAwarenessCliPath } from '../assets.js';
import type { PiContext } from '../types.js';
import { PERSISTENT_AWARENESS_DISABLED_MESSAGE } from './storage-policy.js';

/** Bind model shell calls to the same runtime, identity and store as native Pi events. */
export function buildAwarenessCliEnvironment(ctx?: PiContext): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env.OCTOCODE_AWARENESS_DB;
  delete env.OCTOCODE_AWARENESS_CLI;
  delete env.OCTOCODE_AWARENESS_WORKSPACE;
  if (!isPersistentStorageEnabled()) return env;
  try {
    env.OCTOCODE_AWARENESS_CLI = resolveAwarenessCliPath();
  } catch {
    return env; // Ordinary shell work remains available if the dependency is missing.
  }
  const { client: bindings, identity } = resolveAwarenessHostBindings(ctx);
  env.OCTOCODE_NODE = process.execPath;
  env.OCTOCODE_AGENT_ID = identity.agentId;
  env.OCTOCODE_AGENT_NAME = identity.name;
  env.OCTOCODE_AGENT_HOST = identity.metadata.host;
  if (identity.metadata.vendor) env.OCTOCODE_AGENT_VENDOR = identity.metadata.vendor;
  else delete env.OCTOCODE_AGENT_VENDOR;
  env.OCTOCODE_AWARENESS_WORKSPACE = bindings.workspace;
  env.OCTOCODE_AWARENESS_DB = bindings.database;
  return env;
}

/** Host facts only: operating policy and command catalog are owned by Awareness. */
export function renderAwarenessCliContext(ctx?: PiContext): string {
  if (!isPersistentStorageEnabled()) return `<awareness_cli_runtime>\n${PERSISTENT_AWARENESS_DISABLED_MESSAGE}. Do not use durable Awareness commands; keep work in session state.\n</awareness_cli_runtime>`;
  const env = buildAwarenessCliEnvironment(ctx);
  if (!env.OCTOCODE_AWARENESS_CLI) return '<awareness_cli_runtime>Awareness CLI dependency is unavailable. Report the missing runtime before relying on shared coordination.</awareness_cli_runtime>';
  return [
    '<awareness_cli_runtime>',
    'Awareness skill is bundled. This external tool set lacks the native facade, so explicit coordination uses the bound CLI through bash.',
    'bash inherits the host-bound identity, workspace, database, and runtime through environment variables.',
    'Runner: "$OCTOCODE_NODE" "$OCTOCODE_AWARENESS_CLI" --db "$OCTOCODE_AWARENESS_DB" <command> [options]. Pass --workspace "$OCTOCODE_AWARENESS_WORKSPACE" on scoped commands and --agent-id "$OCTOCODE_AGENT_ID" when required. Use this installed runner for the npx commands in the Awareness guide.',
    '</awareness_cli_runtime>',
  ].join('\n');
}
