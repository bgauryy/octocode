// ─── Agent naming ─────────────────────────────────────────────────────────────

/** Compact funny codename pool (sea creature × scientist, mirrors the harness pool). */
const AGENT_NAME_POOL = [
  'squidJobs', 'inkstein', 'octoDarwin', 'jellyTorvalds', 'calamariCurie',
  'crabBohr', 'seahorseHopper', 'lobsterLovelace', 'morayTuring', 'shellKnuth',
  'stingraySagan', 'blobfishBabbage', 'cuttlefishCook', 'narwhalKnuth', 'pinchyPauli',
  'eelCerf', 'snappyCopernicus', 'mantaGates', 'zappyTesla', 'starfishStallman',
] as const;

export type AgentHost =
  | 'claude'
  | 'cursor'
  | 'codex'
  | 'opencode'
  | 'vscode'
  | 'zed'
  | 'jetbrains'
  | 'octo'
  | 'agent';

/** Name tag per host — recognizable runner, sea pun where it writes itself. */
const HOST_NAME_TAG: Record<AgentHost, string> = {
  claude: 'clawde',
  cursor: 'cursea',
  codex: 'codex',
  opencode: 'opencode',
  vscode: 'vscode',
  zed: 'zed',
  jetbrains: 'jetbrains',
  octo: 'octo',
  agent: 'agent',
};

/**
 * Detect the running host from the environment so generated agent names tell
 * you WHICH runner joined the shared registry. `OCTOCODE_AGENT_HOST` wins (the
 * Octocode harness sets it, so its sessions tag 'octo' even when launched from
 * a Claude Code or Cursor terminal whose env vars are inherited). Recognition
 * is best-effort: only tag a host on a reliable signal; anything unrecognized
 * falls back to the generic 'agent' (never a wrong guess).
 */
export function detectAgentHost(env: NodeJS.ProcessEnv = process.env): AgentHost {
  const override = String(env['OCTOCODE_AGENT_HOST'] ?? '').toLowerCase();
  if (override === 'octocode' || override === 'octocode-agent') return 'octo';
  if (Object.prototype.hasOwnProperty.call(HOST_NAME_TAG, override)) return override as AgentHost;

  // Agent CLIs (checked before terminal/IDE signals, which forks also set).
  if (env['CLAUDECODE'] || env['CLAUDE_CODE_ENTRYPOINT']) return 'claude';
  if (env['CURSOR_TRACE_ID'] || env['CURSOR_AGENT']) return 'cursor';
  if (env['CODEX_THREAD_ID'] || env['CODEX_SANDBOX']) return 'codex';
  if (env['OPENCODE'] || env['OPENCODE_CONFIG'] || env['OPENCODE_BIN_PATH']) return 'opencode';
  // IDEs with a distinguishable terminal signal.
  if (env['ZED_TERM']) return 'zed';
  if (String(env['TERMINAL_EMULATOR'] ?? '').includes('JetBrains')) return 'jetbrains';
  if (env['TERM_PROGRAM'] === 'vscode') return 'vscode';
  return 'agent';
}

/**
 * A funny, host-tagged agent name, e.g. `clawde-squidJobs` (Claude Code),
 * `cursea-crabBohr` (Cursor), `octo-inkstein` (Octocode harness). Used as the
 * default when `agent join` is called without --name so a registry shared by
 * several runners stays legible at a glance.
 */
export function generateAgentName(env: NodeJS.ProcessEnv = process.env): string {
  const funny = AGENT_NAME_POOL[Math.floor(Math.random() * AGENT_NAME_POOL.length)];
  return `${HOST_NAME_TAG[detectAgentHost(env)]}-${funny}`;
}
