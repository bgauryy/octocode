import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash, randomBytes } from 'node:crypto';
import {
  getMcpEnablement,
  listMcpOverrides,
  listSkillOverrides,
  normalizeSkillKey,
  setSkillEnabled,
} from '@octocodeai/agent-contracts/mcp-state';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/agent-contracts/permissions';
import { openOctocodeDb } from './storage-policy.js';
import type { PiCommand, PiContext, PiInstance, SkillInfo } from '../types.js';
import { getOctocodeHome } from '@octocodeai/config';
import { extensionTmpRoot } from '../extension-paths.js';
import { escapeHtml, renderOctocodePage } from '../tui/html-page.js';
import { loadMcpConfig, type McpServerConfig } from './mcp-config.js';
import { getMcpDiscoverySnapshot, getMcpPromptArtifactStatus, handleMcpAction, isMcpServerConnected } from './mcp-tool.js';
import { runtimeStoreFor } from './runtime-renderer.js';
import { hasStoredMcpOAuthTokens } from './mcp-oauth.js';
import { discoverSkillStates } from './skill-tool.js';
import { serveDirectory, unmount } from './local-server.js';
import { openPlanReview } from './plan-tool.js';
import { openLocalUrl } from './local-url-opener.js';
import { getFooterDensity, setFooterDensity, type FooterDensity } from '../ui-extras.js';
import { getPermissionLevel, setPermissionLevel } from './approval.js';
import { type PermissionLevel } from '@octocodeai/agent-contracts/protocols';
import {
  ContributionRegistry,
  SettingsRegistry,
  SettingsService,
  revision,
  type SettingsSnapshot,
} from '@octocodeai/agent-core';
import { PiSettingsAdapter } from '../adapters/pi-settings-adapter.js';
import { applyDialLevel, EFFORT_LEVELS, getActiveDialLevel, type EffortLevel } from './effort-dial.js';
import { updateOctocodeMetricsUi } from '../extension-ui.js';
import { OCTOCODE_THEME_DARK, OCTOCODE_THEME_LIGHT } from '../ui-extras.js';
import { discoverCodexHookSources, type CodexHookDiscoveryResult } from '../adapters/pi-hook-discovery.js';

export const SETTINGS_HTML_FILE = 'settings.html';

function managerDir(cwd: string): string {
  const key = createHash('sha256').update(path.resolve(cwd)).digest('hex').slice(0, 32);
  return path.join(extensionTmpRoot(), 'settings', key);
}

export type McpManagerAction =
  | { action: 'open-plan' }
  | { action: 'enable' | 'disable'; server: string; tool?: string; scope: 'project' | 'global' }
  | { action: 'add'; server: string; scope: 'project' | 'global'; config: Record<string, unknown> }
  | { action: 'remove' | 'restart' | 'connect' | 'retry'; server: string; scope: 'project' | 'global' }
  | { action: 'enable-skill' | 'disable-skill'; skill: string; scope: 'project' | 'global' }
  | { action: 'set-footer-density'; density: FooterDensity; expectedRevision?: string }
  | { action: 'set-permission-level'; level: PermissionLevel; expectedRevision?: string }
  | { action: 'set-effort'; level: EffortLevel; expectedRevision?: string }
  | { action: 'set-theme'; theme: 'dark' | 'light'; expectedRevision?: string }
  | { action: 'review-hook'; source: string; hash: string; expectedRevision?: string }
  | { action: 'enable-hook' | 'disable-hook'; source: string; expectedRevision?: string };

const SERVER_NAME = /^[A-Za-z0-9_.-]{1,64}$/;
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;
const HTTP_HEADER_NAME = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
const SKILL_NAME = /^[^\0\r\n]{1,160}$/;

export function parseMcpManagerAction(raw: unknown): McpManagerAction {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid MCP action');
  const value = raw as Record<string, unknown>;
  for (const key of Object.keys(value)) {
    if (!['action', 'server', 'scope', 'tool', 'config', 'skill', 'density', 'level', 'theme', 'source', 'hash', 'expectedRevision'].includes(key)) throw new Error(`Unsupported settings action field: ${key}`);
  }
  const action = value['action'];
  if (action === 'open-plan') return { action };
  const expectedRevision = typeof value['expectedRevision'] === 'string' ? value['expectedRevision'] : undefined;
  const server = value['server'];
  if (action === 'set-effort') {
    if (!EFFORT_LEVELS.includes(value['level'] as EffortLevel)) throw new Error('Invalid effort level');
    return { action, level: value['level'] as EffortLevel, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-theme') {
    if (value['theme'] !== 'dark' && value['theme'] !== 'light') throw new Error('Invalid theme');
    return { action, theme: value['theme'], ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-footer-density') {
    if (!['compact', 'default', 'full'].includes(String(value['density']))) throw new Error('Invalid footer density');
    return { action, density: value['density'] as FooterDensity, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'set-permission-level') {
    if (!['default', 'relaxed', 'strict'].includes(String(value['level']))) throw new Error('Invalid permission level');
    return { action, level: value['level'] as PermissionLevel, ...(expectedRevision ? { expectedRevision } : {}) };
  }
  if (action === 'review-hook' || action === 'enable-hook' || action === 'disable-hook') {
    if (typeof value['source'] !== 'string' || value['source'].length > 2048) throw new Error('Invalid hook source');
    if (action === 'review-hook' && (typeof value['hash'] !== 'string' || !/^[a-f0-9]{64}$/.test(value['hash']))) throw new Error('Invalid hook review hash');
    return {
      action,
      source: value['source'],
      ...(action === 'review-hook' ? { hash: value['hash'] as string } : {}),
      ...(expectedRevision ? { expectedRevision } : {}),
    } as McpManagerAction;
  }
  if (value['scope'] !== undefined && value['scope'] !== 'project' && value['scope'] !== 'global') throw new Error('Invalid MCP scope');
  const scope = value['scope'] === 'global' ? 'global' : 'project';
  if (action === 'enable-skill' || action === 'disable-skill') {
    const skill = value['skill'];
    if (typeof skill !== 'string' || !SKILL_NAME.test(skill)) throw new Error('Invalid skill name');
    return { action, skill, scope };
  }
  if (!['enable', 'disable', 'add', 'remove', 'restart', 'connect', 'retry'].includes(String(action))) throw new Error('Unsupported settings action');
  if (typeof server !== 'string' || !SERVER_NAME.test(server)) throw new Error('Invalid MCP server');
  if (action === 'enable' || action === 'disable') {
    const tool = value['tool'];
    if (tool !== undefined && (typeof tool !== 'string' || !SERVER_NAME.test(tool))) throw new Error('Invalid MCP tool');
    return { action, server, scope, ...(typeof tool === 'string' ? { tool } : {}) };
  }
  if (action === 'add') {
    if (!value['config'] || typeof value['config'] !== 'object' || Array.isArray(value['config'])) throw new Error('MCP add requires config');
    const config = value['config'] as Record<string, unknown>;
    const allowed = new Set(['command', 'args', 'cwd', 'url', 'timeoutMs', 'description', 'envRefs', 'headerRefs', 'auth']);
    for (const key of Object.keys(config)) if (!allowed.has(key)) throw new Error(`Unsupported MCP config field: ${key}`);
    for (const field of ['envRefs', 'headerRefs'] as const) {
      const refs = config[field];
      if (refs === undefined) continue;
      if (!refs || typeof refs !== 'object' || Array.isArray(refs)) throw new Error(`${field} must be an object`);
      for (const [destination, source] of Object.entries(refs as Record<string, unknown>)) {
        const validDestination = field === 'envRefs' ? ENV_NAME.test(destination) : HTTP_HEADER_NAME.test(destination);
        if (!validDestination) throw new Error(`Invalid ${field} destination: ${destination}`);
        if (typeof source !== 'string' || !ENV_NAME.test(source)) throw new Error(`${field} values must be environment variable names`);
      }
    }
    return { action, server, scope, config };
  }
  return { action, server, scope } as McpManagerAction;
}

const settingsAdapters = new Map<string, PiSettingsAdapter>();
const hookDiscovery = new Map<string, CodexHookDiscoveryResult>();
const pluginContributions = new ContributionRegistry();

function settingsAdapter(ctx?: PiContext): PiSettingsAdapter {
  const key = path.resolve(ctx?.cwd ?? process.cwd());
  const cached = settingsAdapters.get(key);
  if (cached) return cached;
  const registry = new SettingsRegistry();
  registry.register({ key: 'runtime.footer-density', schemaVersion: 1, section: 'Appearance', order: 10, kind: { type: 'enum', values: ['compact', 'default', 'full'] }, scopes: ['session'], defaultValue: getFooterDensity(), mutability: 'editable', application: 'immediate', visibility: 'public', owner: 'pi-extension', documentation: 'docs/SETTINGS.md' });
  registry.register({ key: 'runtime.permission-level', schemaVersion: 1, section: 'Runtime', order: 10, kind: { type: 'enum', values: ['default', 'relaxed', 'strict'] }, scopes: ['session'], defaultValue: getPermissionLevel(), mutability: 'editable', application: 'immediate', visibility: 'public', owner: 'pi-extension', documentation: 'docs/SETTINGS.md' });
  registry.register({ key: 'models.active', schemaVersion: 1, section: 'Models', order: 10, kind: { type: 'object' }, scopes: ['imported'], defaultValue: { providerId: ctx?.model?.provider ?? null, modelId: ctx?.model?.id ?? null }, mutability: 'read-only', application: 'next-session', visibility: 'public', owner: 'pi-extension', documentation: 'docs/SETTINGS.md', classificationReason: 'Active Pi model is a compatibility projection; canonical defaults are edited by agent-core.' });
  registry.register({ key: 'runtime.theme', schemaVersion: 1, section: 'Appearance', order: 20, kind: { type: 'enum', values: ['host', 'dark', 'light'] }, scopes: ['session'], defaultValue: 'host', mutability: 'editable', application: 'immediate', visibility: 'public', owner: 'pi-extension', documentation: 'docs/SETTINGS.md' });
  registry.register({ key: 'runtime.effort', schemaVersion: 1, section: 'Runtime', order: 20, kind: { type: 'enum', values: ['host', ...EFFORT_LEVELS] }, scopes: ['session'], defaultValue: getActiveDialLevel() ?? 'host', mutability: 'editable', application: 'immediate', visibility: 'public', owner: 'pi-extension', documentation: 'docs/SETTINGS.md' });
  const adapter = new PiSettingsAdapter(new SettingsService(registry));
  adapter.subscribe((result) => {
    if (result.effectiveValue?.key === 'runtime.footer-density') setFooterDensity(result.effectiveValue.value as FooterDensity);
    if (result.effectiveValue?.key === 'runtime.permission-level') setPermissionLevel(result.effectiveValue.value as PermissionLevel);
  });
  settingsAdapters.set(key, adapter);
  return adapter;
}

function hooksFor(ctx?: PiContext): CodexHookDiscoveryResult {
  const key = path.resolve(ctx?.cwd ?? process.cwd());
  const cached = hookDiscovery.get(key);
  if (cached) return cached;
  const discovered = discoverCodexHookSources({ workspace: key });
  hookDiscovery.set(key, discovered);
  return discovered;
}

export async function applyMcpManagerAction(action: McpManagerAction, ctx?: PiContext, pi?: PiInstance): Promise<void> {
  if (action.action === 'open-plan') {
    const url = await openPlanReview(ctx);
    if (!url) throw new Error('Could not open the current plan');
    return;
  }
  if (action.action === 'set-theme' || action.action === 'set-effort') {
    if (action.expectedRevision && action.expectedRevision !== settingsAdapter(ctx).snapshot().revision) throw new Error('Configuration changed since this page was generated');
    if (action.action === 'set-theme') {
      if (!ctx?.ui?.setTheme) throw new Error('Theme changes are unavailable in this host');
      const result = ctx.ui.setTheme(action.theme === 'dark' ? OCTOCODE_THEME_DARK : OCTOCODE_THEME_LIGHT);
      if (result && !result.success) throw new Error(result.error ?? 'Could not apply theme');
    } else {
      if (!pi?.setThinkingLevel) throw new Error('Effort changes are unavailable in this host');
      await applyDialLevel(pi, ctx, action.level, { persist: false });
    }
    const adapter = settingsAdapter(ctx);
    const result = await adapter.mutate({ protocolVersion: 1, requestId: randomBytes(12).toString('hex'), action: 'set', scope: 'session', expectedRevision: revision(action.expectedRevision ?? adapter.snapshot().revision), payload: { key: action.action === 'set-theme' ? 'runtime.theme' : 'runtime.effort', value: action.action === 'set-theme' ? action.theme : action.level } });
    if (!result.ok) throw new Error(result.error.message);
    updateOctocodeMetricsUi(ctx);
    return;
  }
  if (action.action === 'set-footer-density') {
    const adapter = settingsAdapter(ctx);
    const result = await adapter.mutate({ protocolVersion: 1, requestId: randomBytes(12).toString('hex'), action: 'set', scope: 'session', expectedRevision: revision(action.expectedRevision ?? adapter.snapshot().revision), payload: { key: 'runtime.footer-density', value: action.density } });
    if (!result.ok) throw new Error(result.error.message);
    runtimeStoreFor(ctx)?.getState().announce(`Footer density: ${action.density}`, 'info');
    return;
  }
  if (action.action === 'set-permission-level') {
    const adapter = settingsAdapter(ctx);
    const result = await adapter.mutate({ protocolVersion: 1, requestId: randomBytes(12).toString('hex'), action: 'set', scope: 'session', expectedRevision: revision(action.expectedRevision ?? adapter.snapshot().revision), payload: { key: 'runtime.permission-level', value: action.level } });
    if (!result.ok) throw new Error(result.error.message);
    runtimeStoreFor(ctx)?.getState().announce(`Permission level: ${action.level}`, action.level === 'relaxed' ? 'warning' : 'info');
    return;
  }
  if (action.action === 'review-hook' || action.action === 'enable-hook' || action.action === 'disable-hook') {
    const hooks = hooksFor(ctx);
    const snapshot = hooks.catalog.snapshot();
    if (action.expectedRevision && action.expectedRevision !== snapshot.revision) throw new Error('Hook catalog changed since this page was generated');
    const entry = snapshot.entries.find((candidate) => candidate.source.id === action.source);
    if (!entry) throw new Error('Unknown hook source');
    if (entry.source.scope === 'workspace' && ctx?.isProjectTrusted && !(await ctx.isProjectTrusted())) throw new Error('Workspace hook review refused because the workspace is not trusted');
    if (action.action === 'review-hook') hooks.catalog.review(action.source, action.hash);
    else hooks.catalog.setEnabled(action.source, action.action === 'enable-hook');
    return;
  }
  if (action.action === 'enable-skill' || action.action === 'disable-skill') {
    if (action.scope === 'project' && ctx?.isProjectTrusted && !(await ctx.isProjectTrusted())) {
      throw new Error('Project skill override refused because the workspace is not trusted');
    }
    const scopeKey = action.scope === 'global' ? '*' : path.resolve(ctx?.cwd ?? process.cwd());
    setSkillEnabled(openOctocodeDb(), scopeKey, action.skill, action.action === 'enable-skill');
    const state = runtimeStoreFor(ctx)?.getState();
    if (state?.context.status === 'frozen') state.setContext({ status: 'stale' });
    state?.announce(`Skill ${action.skill} ${action.action === 'enable-skill' ? 'enabled' : 'disabled'} for ${action.scope}. Start /new to refresh the frozen prompt.`, 'info');
    return;
  }
  const toolAction = action.action === 'connect' || action.action === 'retry' ? { ...action, action: 'restart' as const } : action;
  const response = await handleMcpAction(toolAction, undefined, ctx, { trustedBrowserAction: true });
  if (response.isError) throw new Error((response.content[0] as { text?: string } | undefined)?.text ?? 'MCP update failed');
}

function safeConfig(config: McpServerConfig): Record<string, unknown> {
  const safeUrl = (() => {
    if (!config.url) return undefined;
    try {
      const value = new URL(config.url);
      value.username = '';
      value.password = '';
      value.search = '';
      value.hash = '';
      return value.toString();
    } catch { return '(invalid URL)'; }
  })();
  return config.transport === 'http' || config.url
    ? {
        transport: 'streamable-http',
        url: safeUrl,
        headerKeys: [...Object.keys(config.headers ?? {}), ...Object.keys(config.headerRefs ?? {})].sort(),
        headerRefs: config.headerRefs ?? {},
        bearerTokenEnvVar: config.bearerTokenEnvVar,
        auth: config.auth ?? 'none',
        timeoutMs: config.timeoutMs,
        description: config.description,
      }
    : {
        transport: 'stdio',
        command: config.command,
        ...(config.discovered ? { argumentCount: config.args?.length ?? 0 } : { args: config.args ?? [] }),
        cwd: config.cwd,
        envKeys: [...Object.keys(config.env ?? {}), ...Object.keys(config.envRefs ?? {})].sort(),
        envRefs: config.envRefs ?? {},
        timeoutMs: config.timeoutMs,
        description: config.description,
      };
}

export async function renderMcpManagerPage(ctx?: PiContext, actionToken = '', piSkills?: SkillInfo[], commands: readonly PiCommand[] = [], pi?: PiInstance): Promise<string> {
  const cwd = path.resolve(ctx?.cwd ?? process.cwd());
  const skills = discoverSkillStates(cwd, piSkills);
  const loaded = await loadMcpConfig(ctx);
  const discovery = await getMcpDiscoverySnapshot(ctx);
  const artifacts = getMcpPromptArtifactStatus(ctx);
  const contextState = runtimeStoreFor(ctx)?.getState().context;
  const discoveredToolCount = discovery.servers.reduce((total, server) => total + (server.tools?.length ?? 0), 0);
  const catalog = new Map(discovery.servers.map((server) => [server.name, server]));
  let overrides = { servers: [] as unknown[], tools: [] as unknown[], skills: [] as unknown[] };
  let skillOverrides: ReturnType<typeof listSkillOverrides> = [];
  let db: ReturnType<typeof openOctocodeDb> | undefined;
  try {
    db = openOctocodeDb();
    overrides = { ...listMcpOverrides(db, cwd), skills: [] };
    skillOverrides = listSkillOverrides(db, cwd);
    overrides.skills = skillOverrides;
  } catch { /* page stays usable without DB diagnostics */ }
  const oauthHealth = new Map<string, boolean>();
  await Promise.all([...loaded.configuredServers.entries()].map(async ([name, config]) => {
    if (config.auth === 'oauth' && config.url) {
      oauthHealth.set(name, await hasStoredMcpOAuthTokens(name, config.url).catch(() => false));
    }
  }));
  const rows = [...loaded.configuredServers.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([name, config]) => {
    const enabled = loaded.servers.has(name);
    const source = loaded.serverSources.get(name);
    const editScope = source?.scope === 'global' || source?.scope === 'discovered-user' ? 'global' : 'project';
    const imported = Boolean(config.discovered || source?.readOnly);
    const connected = isMcpServerConnected(name);
    const authHealth = config.auth === 'oauth' ? (oauthHealth.get(name) ? 'authorized' : 'authorization required') : 'not required';
    const tools = catalog.get(name)?.tools ?? [];
    const toolRows = tools.map((tool) => {
      const toolEnabled = !db ? !imported : getMcpEnablement(db, cwd, name, tool.name, !imported);
      const action = toolEnabled ? 'disable' : 'enable';
      return `<div class="tool-row"><span><code>${escapeHtml(tool.name)}</code><small>${escapeHtml(tool.description)}</small></span><button class="${toolEnabled ? '' : 'primary'}" data-action="${action}" data-server="${escapeHtml(name)}" data-tool="${escapeHtml(tool.name)}">${toolEnabled ? 'Disable' : 'Enable'}</button></div>`;
    }).join('');
    const encodedConfig = Buffer.from(JSON.stringify({
      name,
      scope: editScope,
      ...safeConfig(config),
      transport: config.transport === 'http' || config.url ? 'http' : 'stdio',
    }), 'utf8').toString('base64');
    const sourceLabel = imported ? `Discovered from ${config.discovered?.host ?? source?.host ?? 'external host'}` : `Managed ${source?.scope ?? 'unknown'}`;
    return `<section class="server-card" data-search="${escapeHtml(`${name} ${config.description ?? ''} ${sourceLabel}`.toLowerCase())}" data-state="${enabled ? 'enabled' : 'disabled'}" data-origin="${imported ? 'discovered' : 'managed'}">
      <div class="server-head"><div><p class="server-origin">${escapeHtml(sourceLabel)}</p><h3>${escapeHtml(name)}</h3></div><div class="server-badges"><span class="badge ${connected ? 'on' : ''}">${connected ? 'connected' : 'offline'}</span><span class="badge ${enabled ? 'on' : ''}">${enabled ? 'enabled' : 'disabled'}</span></div></div>
      <p class="server-description">${escapeHtml(config.description ?? (imported ? 'Found automatically. Disabled until you explicitly enable it.' : 'No description provided.'))}</p>
      <div class="server-facts"><span><b>${config.transport === 'http' || config.url ? 'HTTP' : 'STDIO'}</b> transport</span><span><b>${tools.length}</b> cached tool${tools.length === 1 ? '' : 's'}</span><span><b>${escapeHtml(authHealth)}</b> OAuth</span></div>
      <details><summary>Configuration and source</summary><p class="muted">Effective scope: ${escapeHtml(source?.scope ?? 'unknown')} · <code>${escapeHtml(source?.path ?? 'unknown')}</code></p><pre>${escapeHtml(JSON.stringify(safeConfig(config), null, 2))}</pre></details>
      ${tools.length ? `<details class="tool-list"><summary>${tools.length} tools · ${imported ? 'disabled by default' : 'enablement controls'}</summary><div>${toolRows}</div></details>` : '<p class="empty-note">No cached tools yet. Enable the server, then connect to discover its catalog.</p>'}
      <div class="reply-actions">${imported ? '' : `<button data-action="edit" data-config="${encodedConfig}">Edit</button>`}<button class="${enabled ? '' : 'primary'}" data-action="${enabled ? 'disable' : 'enable'}" data-server="${escapeHtml(name)}" data-scope="${editScope}">${enabled ? 'Disable' : imported ? 'Enable import' : 'Enable'}</button>${enabled ? `<button data-action="restart" data-server="${escapeHtml(name)}" data-scope="${editScope}">${config.auth === 'oauth' && !oauthHealth.get(name) ? 'Authorize / connect' : 'Connect / retry'}</button>` : ''}${name === 'octocode' || imported ? '' : `<button data-action="remove" data-server="${escapeHtml(name)}" data-scope="${editScope}">Remove</button>`}</div>
    </section>`;
  }).join('');
  const sources = loaded.sources.map((source) => `<li><span class="badge ${source.trusted ? 'on' : ''}">${escapeHtml(source.host ?? source.scope)}</span><span><code>${escapeHtml(source.path)}</code><small>${source.readOnly ? 'discovered · read-only · disabled by default' : source.trusted ? 'active definition source' : 'untrusted · not imported'}</small></span></li>`).join('');
  const skillRows = skills.map((skill) => {
    const key = normalizeSkillKey(skill.name);
    const workspaceOverride = skillOverrides.find((override) => override.scopeKey === cwd && override.skillKey === key);
    const globalOverride = skillOverrides.find((override) => override.scopeKey === '*' && override.skillKey === key);
    const effectiveSource = workspaceOverride ? 'workspace override' : globalOverride ? 'global override' : 'default';
    const globalSelected = !workspaceOverride && Boolean(globalOverride);
    return `<article class="skill-card" data-skill-search="${escapeHtml(`${skill.name} ${skill.description} ${skill.source}`.toLowerCase())}" data-skill-state="${skill.enabled ? 'enabled' : 'disabled'}"><div class="skill-card-head"><div><span class="badge">${escapeHtml(skill.source)}</span><h3>${escapeHtml(skill.name)}</h3></div><span class="badge ${skill.enabled ? 'on' : ''}">${skill.enabled ? 'enabled' : 'disabled'}</span></div><p>${escapeHtml(skill.description || '(no description)')}</p><details><summary>Source and effective state</summary><code>${escapeHtml(skill.path)}</code><small>${escapeHtml(effectiveSource)}</small></details><div class="skill-actions"><select aria-label="Scope for ${escapeHtml(skill.name)}" data-skill-scope><option value="project"${globalSelected ? '' : ' selected'}>This workspace</option><option value="global"${globalSelected ? ' selected' : ''}>All workspaces</option></select><button class="${skill.enabled ? '' : 'primary'}" data-action="${skill.enabled ? 'disable-skill' : 'enable-skill'}" data-skill="${escapeHtml(skill.name)}">${skill.enabled ? 'Disable skill' : 'Enable skill'}</button></div></article>`;
  }).join('');
  const commandRows = commands.map((command) => {
    const description = command.description?.trim() || 'No description provided.';
    const source = command.source || 'extension';
    const sourceInfo = command.sourceInfo;
    const search = `${command.name} ${description} ${source} ${sourceInfo?.source ?? ''}`.toLowerCase();
    return `<article class="command-card" data-command-search="${escapeHtml(search)}" data-command-source="${escapeHtml(source)}"><div class="command-card-head"><code>/${escapeHtml(command.name)}</code><span class="badge ${source === 'extension' ? 'on' : ''}">${escapeHtml(source)}</span></div><p>${escapeHtml(description)}</p>${sourceInfo ? `<details><summary>Registration source</summary><code>${escapeHtml(sourceInfo.path)}</code><small>${escapeHtml(`${sourceInfo.source} · ${sourceInfo.scope} · ${sourceInfo.origin}`)}</small></details>` : ''}</article>`;
  }).join('');
  const promptState = contextState?.status ?? artifacts.status;
  const modeSummary = artifacts.mode === 'compact'
    ? 'Compact mcp.md guide is injected; exact catalog.json remains private for validation.'
    : 'Exact enabled descriptions and input schemas from catalog.json are injected; mcp.md is ignored.';
  const importedCount = [...loaded.configuredServers.values()].filter((config) => config.discovered).length;
  const enabledSkillCount = skills.filter((skill) => skill.enabled).length;
  const footerDensity = getFooterDensity();
  const permissionLevel = getPermissionLevel();
  const canonicalSettings: SettingsSnapshot = settingsAdapter(ctx).snapshot();
  const settingsRevision = canonicalSettings.revision;
  const settingValue = (key: string): unknown => canonicalSettings.values.find((value) => value.key === key)?.value;
  const canonicalFooterDensity = settingValue('runtime.footer-density') as FooterDensity ?? footerDensity;
  const canonicalPermissionLevel = settingValue('runtime.permission-level') as PermissionLevel ?? permissionLevel;
  const hooks = hooksFor(ctx);
  const hookSnapshot = hooks.catalog.snapshot();
  const hookRows = hookSnapshot.entries.map((entry) => `<div class="row"><span><strong>${escapeHtml(entry.source.scope)}</strong> · <code>${escapeHtml(entry.source.provenance)}</code><br><small>${escapeHtml(entry.source.normalizedHash)} · ${entry.source.managed ? 'managed' : 'exact-definition review'}</small></span><span class="reply-actions"><span class="badge ${entry.executable ? 'on' : ''}">${entry.executable ? 'trusted' : 'review required'}</span>${entry.executable ? '' : `<button data-action="review-hook" data-source="${escapeHtml(entry.source.id)}" data-hash="${entry.source.normalizedHash}">Review exact hash</button>`}<button data-action="${entry.enabled ? 'disable-hook' : 'enable-hook'}" data-source="${escapeHtml(entry.source.id)}">${entry.enabled ? 'Disable' : 'Enable'}</button></span></div>`).join('');
  const modelSources = [path.join(getOctocodeHome(), 'agent', 'models.json'), path.join(cwd, '.octocode', 'agent', 'models.json'), path.join(os.homedir(), '.pi', 'agent', 'models.json'), path.join(cwd, '.pi', 'models.json')];
  const pluginSnapshot = pluginContributions.list();
  const bodyHtml = `<style>
      .settings-shell>*,.row>*,.server-head>*,.skill-card-head>*{min-width:0}.row{gap:.8rem;flex-wrap:wrap}.row code,.row small,.source-list code,.source-list small{overflow-wrap:anywhere;word-break:break-word}.settings-shell section{min-width:0}.row .reply-actions{flex-wrap:wrap}
      .control-hero{overflow:hidden;position:relative;background:linear-gradient(125deg,#152A40,#233D59);color:white;border:0;box-shadow:var(--shadow);padding:clamp(1.3rem,3vw,2rem)}
      .control-hero::after{content:"";position:absolute;width:250px;height:250px;border-radius:50%;right:-80px;top:-130px;background:linear-gradient(135deg,var(--violet),var(--cyan));opacity:.42}.control-hero h2{color:#FFB47D}.control-hero p{max-width:700px;color:#D8E6F2;margin:.2rem 0 1.25rem}
      .stats{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:.7rem;position:relative;z-index:1}.stat{padding:.8rem 1rem;border:1px solid rgba(255,255,255,.16);border-radius:13px;background:rgba(255,255,255,.08)}.stat b{display:block;color:white;font-size:1.45rem;line-height:1}.stat span{font-size:.72rem;color:#BED0DF;text-transform:uppercase;letter-spacing:.08em}
      .settings-shell{display:grid;grid-template-columns:210px minmax(0,1fr);gap:1rem;align-items:start}.settings-nav{position:sticky;top:1rem;background:rgba(255,255,255,.78);border:1px solid var(--line);border-radius:16px;padding:.75rem}.settings-nav a{display:block;padding:.55rem .65rem;border-radius:9px;color:var(--muted);text-decoration:none;font-weight:700;font-size:.82rem}.settings-nav a:hover{color:var(--violet);background:#F2EEFF}.settings-nav .nav-tip{margin:.7rem .55rem .3rem;padding-top:.7rem;border-top:1px solid var(--line);font-size:.72rem;color:var(--muted)}
      .section-heading{display:flex;justify-content:space-between;gap:1rem;align-items:end;margin:1.5rem .2rem .75rem}.section-heading:first-child{margin-top:0}.section-heading h2{margin:0;color:var(--ink);font-size:1.15rem;letter-spacing:-.02em;text-transform:none}.section-heading p{margin:0;color:var(--muted);font-size:.82rem}
      .filterbar{display:grid;grid-template-columns:minmax(0,1fr) auto auto;gap:.55rem;margin-bottom:.75rem}.filterbar input{padding:.72rem .85rem}.filterbar button.active{color:white;background:var(--violet);border-color:var(--violet)}
      .server-card{padding:1.1rem 1.2rem}.server-head{display:flex;justify-content:space-between;gap:1rem;align-items:start}.server-head h3{margin:.08rem 0 0;font-size:1.05rem}.server-origin{margin:0;color:var(--violet);font:700 .66rem/1.3 ui-monospace,monospace;text-transform:uppercase;letter-spacing:.1em}.server-badges{display:flex;gap:.35rem;flex-wrap:wrap;justify-content:flex-end}.server-description{margin:.65rem 0;color:var(--muted)}.server-facts{display:flex;flex-wrap:wrap;gap:.45rem;margin:.65rem 0}.server-facts span{padding:.3rem .55rem;border-radius:8px;background:var(--panel-soft);border:1px solid var(--line);font-size:.74rem;color:var(--muted)}.server-facts b{color:var(--ink)}
      .tool-list{margin-top:.65rem}.tool-row{display:flex;justify-content:space-between;align-items:center;gap:1rem;padding:.6rem 0;border-bottom:1px solid var(--line)}.tool-row:last-child{border-bottom:0}.tool-row span{display:grid;gap:.15rem}.tool-row small,.empty-note{color:var(--muted)}
      .source-list{list-style:none;padding:0;margin:0}.source-list li{display:flex;align-items:flex-start;gap:.65rem;padding:.6rem 0;border-bottom:1px solid var(--line)}.source-list li:last-child{border:0}.source-list small{display:block;color:var(--muted);margin-top:.2rem}.editor-grid{display:grid;grid-template-columns:1fr 1fr;gap:.8rem}.editor-grid .wide-field{grid-column:1/-1}.security-note{padding:.8rem 1rem;border-radius:11px;background:#FFF7EE;border:1px solid #FFD7BA;color:#7E4A28;font-size:.82rem}.toast{position:fixed;right:1rem;bottom:1rem;max-width:360px;padding:.75rem 1rem;border-radius:11px;background:var(--ink);color:white;box-shadow:var(--shadow);z-index:10}.hidden{display:none!important}
      .skill-toolbar{display:grid;grid-template-columns:minmax(0,1fr) auto auto auto;gap:.55rem;margin-bottom:.75rem}.skill-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:.75rem}.skill-card{background:white;border:1px solid var(--line);border-radius:15px;padding:1rem;box-shadow:0 4px 16px rgba(31,65,96,.04)}.skill-card-head{display:flex;align-items:start;justify-content:space-between;gap:.7rem}.skill-card h3{margin:.4rem 0 0;font-size:.96rem}.skill-card p{color:var(--muted);font-size:.82rem;display:-webkit-box;-webkit-line-clamp:3;-webkit-box-orient:vertical;overflow:hidden;min-height:3.9em}.skill-card details code{display:block;overflow:hidden;text-overflow:ellipsis;margin:.45rem 0}.skill-card details small{color:var(--muted)}.skill-actions{display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.5rem;margin-top:.8rem}.skill-actions select{padding:.48rem .6rem;font-size:.76rem}
      .command-toolbar{display:grid;grid-template-columns:minmax(0,1fr) repeat(4,auto);gap:.55rem;margin-bottom:.75rem}.command-toolbar button.active{color:white;background:var(--violet);border-color:var(--violet)}.command-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:.65rem}.command-card{background:white;border:1px solid var(--line);border-radius:13px;padding:.85rem 1rem}.command-card-head{display:flex;align-items:center;justify-content:space-between;gap:.75rem}.command-card-head code{font-size:.88rem;color:var(--violet);font-weight:800}.command-card p{color:var(--muted);font-size:.8rem;margin:.55rem 0}.command-card details code{display:block;overflow:hidden;text-overflow:ellipsis;margin:.4rem 0}.command-card details small{color:var(--muted)}
      @media(max-width:1000px){.stats{grid-template-columns:repeat(3,1fr)}}@media(max-width:860px){.settings-shell{grid-template-columns:1fr}.settings-nav{position:static;display:flex;overflow-x:auto}.settings-nav a{white-space:nowrap}.settings-nav .nav-tip{display:none}.stats{grid-template-columns:1fr 1fr}}@media(max-width:700px){.skill-grid,.command-grid{grid-template-columns:1fr}.command-toolbar{grid-template-columns:1fr repeat(2,auto)}}@media(max-width:600px){.filterbar,.editor-grid,.skill-toolbar,.command-toolbar{grid-template-columns:1fr}.editor-grid .wide-field{grid-column:auto}.stats{grid-template-columns:1fr 1fr}.server-head{flex-direction:column}.server-badges{justify-content:flex-start}.skill-actions{grid-template-columns:1fr}}
    </style>
    <section class="control-hero" id="overview"><h2>Your configuration</h2><p>Choose the connections, tools, and skills available to your agent. Display and permission controls apply to this session.</p><p><button data-action="open-plan">Review plan</button></p><div class="stats"><div class="stat"><b>${commands.length}</b><span>live commands</span></div><div class="stat"><b>${loaded.servers.size}</b><span>enabled servers</span></div><div class="stat"><b>${importedCount}</b><span>discovered imports</span></div><div class="stat"><b>${discoveredToolCount}</b><span>known MCP tools</span></div><div class="stat"><b>${enabledSkillCount}/${skills.length}</b><span>enabled skills</span></div></div></section>
    <div class="settings-shell"><nav class="settings-nav" aria-label="Settings sections"><a href="#overview">Overview</a><a href="#runtime">Runtime</a><a href="#appearance">Appearance</a><a href="#models">Models</a><a href="#hooks">Hooks</a><a href="#plugins">Plugins</a><a href="#commands">Commands</a><a href="#connections">Connections</a><a href="#add-server">Add server</a><a href="#sources">Discovery</a><a href="#agent-context">Agent context</a><a href="#skills">Skills</a><a href="#overrides">Overrides</a><a href="#diagnostics">Diagnostics</a><p class="nav-tip">Run <code>/configuration</code> anytime to rebuild this page from the live registry.</p></nav><div>
    <div class="section-heading" id="runtime"><div><h2>Runtime controls</h2><p>Session-scoped display and safety controls. Changes apply immediately.</p></div></div>
    <section data-settings-revision="${settingsRevision}"><div class="row"><span><strong>Footer density</strong><br><small>Choose how much detail appears below the conversation.</small></span><span class="reply-actions">${(['compact', 'default', 'full'] as const).map((density) => `<button data-action="set-footer-density" data-density="${density}"${density === canonicalFooterDensity ? ' class="primary"' : ''}>${density}</button>`).join('')}</span></div><div class="row"><span><strong>Permission level</strong><br><small>Strict asks more often; relaxed permits more actions automatically.</small></span><span class="reply-actions">${(['default', 'relaxed', 'strict'] as const).map((level) => `<button data-action="set-permission-level" data-level="${level}"${level === canonicalPermissionLevel ? ' class="primary"' : ''}>${level}</button>`).join('')}</span></div></section>
    <div class="section-heading" id="appearance"><div><h2>Appearance and effort</h2><p>Changes apply to this session.</p></div></div>
    <section><div class="row"><span><strong>Terminal theme</strong><br><small>Choose a light or dark terminal appearance.</small></span><span class="reply-actions">${(['dark', 'light'] as const).map((theme) => `<button data-action="set-theme" data-theme="${theme}" aria-pressed="${settingValue('runtime.theme') === theme}"${settingValue('runtime.theme') === theme ? ' class="primary"' : ''}${ctx?.ui?.setTheme ? '' : ' disabled'}>${theme}</button>`).join('')}</span></div>
    <div class="row"><span><strong>Effort</strong><br><small>Thinking depth and concurrent workers. Current: ${escapeHtml(getActiveDialLevel() ?? 'host settings')}. ${pi?.setThinkingLevel ? '' : 'Changing effort is unavailable in this host.'}</small></span><span class="reply-actions">${EFFORT_LEVELS.map((level) => `<button data-action="set-effort" data-level="${level}"${level === getActiveDialLevel() ? ' class="primary"' : ''}${pi?.setThinkingLevel ? '' : ' disabled'}>${level}</button>`).join('')}</span></div></section>
    <div class="section-heading" id="models"><div><h2>Models</h2><p>The model used by this session.</p></div></div><section><div class="row"><span><strong>${escapeHtml(ctx?.model?.provider ?? 'unknown provider')} / ${escapeHtml(ctx?.model?.id ?? 'unknown model')}</strong><br><small>Change the active model using the host model selector.</small></span><span class="badge">current session</span></div>${modelSources.map((sourcePath) => `<div class="row"><code>${escapeHtml(sourcePath)}</code><span class="badge ${fs.existsSync(sourcePath) ? 'on' : ''}">${fs.existsSync(sourcePath) ? 'present · import-only' : 'not present'}</span></div>`).join('')}<p class="muted">Model configuration files are shown for reference. This page cannot edit them.</p></section>
    <div class="section-heading" id="hooks"><div><h2>Hooks</h2><p>Codex-compatible lifecycle definitions and safe execution health.</p></div><span>revision ${hookSnapshot.revision}</span></div><section>${hookRows || '<p>No Codex hook sources discovered.</p>'}${hooks.errors.map((error) => `<p class="security-note">${escapeHtml(error.path)}: ${escapeHtml(error.message)}</p>`).join('')}<p class="muted">Enablement and exact-definition trust review are distinct. Workspace sources additionally require current workspace trust.</p></section>
    <div class="section-heading" id="plugins"><div><h2>Plugins</h2><p>Extensions active in this session.</p></div></div><section><div class="row"><span>Registered extensions</span><span class="badge">${pluginSnapshot.length} active</span></div><p class="muted">Reload the host after changing installed extensions.</p></section>
    <div class="section-heading" id="commands"><div><h2>Commands</h2><p>Every public slash command registered in this running session.</p></div><span>${commands.length} available now</span></div>
    <section><div class="command-toolbar"><input id="command-filter" type="search" placeholder="Search commands and descriptions…" aria-label="Search commands"><button class="active" data-command-filter="all">All</button><button data-command-filter="extension">Extension</button><button data-command-filter="skill">Skills</button><button data-command-filter="prompt">Prompts</button></div><div id="command-list" class="command-grid">${commandRows || '<p>No live commands were reported by the host. Reopen settings after command registration completes.</p>'}</div><p class="muted">This snapshot is rebuilt from <code>pi.getCommands()</code> every time you run <code>/configuration</code>; internal commands beginning with <code>_</code> are excluded.</p></section>
    <div class="section-heading" id="connections"><div><h2>MCP connections</h2><p>Managed and system-discovered definitions. Foreign imports are namespaced and disabled by default.</p></div></div>
    <div class="filterbar"><input id="server-filter" type="search" placeholder="Search name, description, or source…" aria-label="Search MCP servers"><button class="active" data-filter="all">All</button><button data-filter="discovered">Discovered</button></div>
    <div id="server-list">${rows || '<section><p>No MCP servers configured or discovered.</p></section>'}</div>
    <div class="section-heading" id="add-server"><div><h2>Add a managed server</h2><p>Definitions are written to canonical Octocode JSON. Values below may reference secrets, never contain them.</p></div></div>
    <section><p class="security-note">Security: enter environment-variable names only. OAuth tokens remain in the OS credential store; raw credentials are never accepted or rendered here.</p><form id="mcp-editor" class="editor-grid">
      <label>Name <input name="server" required pattern="[A-Za-z0-9_.-]{1,64}" placeholder="docs"></label>
      <label>Scope <select name="scope"><option value="project">Project</option><option value="global">Global</option></select></label>
      <label>Transport <select name="transport"><option value="stdio">stdio</option><option value="http">Streamable HTTP</option></select></label>
      <label>Timeout ms <input name="timeoutMs" type="number" min="1000" max="120000" value="30000"></label>
      <label data-transport-field="stdio">Command <input name="command" placeholder="node"></label><label data-transport-field="http" class="hidden">URL <input name="url" type="url" placeholder="https://example.test/mcp"></label>
      <label class="wide-field" data-transport-field="stdio">Arguments, one per line <textarea name="args" rows="3"></textarea></label><label data-transport-field="stdio">Working directory <input name="cwd"></label><label>Description <input name="description"></label>
      <label class="wide-field">Environment references (JSON: destination key → environment variable) <textarea name="envRefs" rows="3" placeholder='{"API_KEY":"MY_MCP_API_KEY"}'></textarea></label>
      <label class="wide-field hidden" data-transport-field="http">Header references (JSON: header → environment variable) <textarea name="headerRefs" rows="3" placeholder='{"Authorization":"MY_MCP_AUTH_HEADER"}'></textarea></label>
      <label class="hidden" data-transport-field="http">Authentication <select name="auth"><option value="none">None / references</option><option value="oauth">OAuth (authorize after saving)</option></select></label>
      <div class="reply-actions"><button class="primary" type="submit">Save and refresh</button><button type="reset">Clear</button></div><p id="mcp-status" class="reply-status wide-field"></p>
    </form></section>
    <div class="section-heading" id="sources"><div><h2>Discovery sources</h2><p>Definitions remain owned by their original files; enablement lives in SQLite.</p></div></div><section><ul class="source-list">${sources}</ul>${loaded.warnings.length ? `<details><summary>${loaded.warnings.length} discovery warning${loaded.warnings.length === 1 ? '' : 's'}</summary><pre>${escapeHtml(loaded.warnings.join('\n'))}</pre></details>` : ''}</section>
    <div class="section-heading" id="agent-context"><div><h2>Agent context</h2><p>What the next agent call will receive.</p></div></div><section><h2>Agent prompt catalog</h2>
      <div class="row"><span><span class="badge on">${escapeHtml(artifacts.mode)}</span> <strong>${escapeHtml(promptState)}</strong></span><span>${artifacts.promptChars.toLocaleString()} prompt chars</span></div>
      <p>${escapeHtml(modeSummary)}</p>
      <p class="muted">Mode source: <code>OCTOCODE_COMPACT_MCP</code> (${artifacts.mode === 'compact' ? 'default/enabled' : 'explicitly disabled'}) · mcp.md: ${escapeHtml(artifacts.guideState)}${artifacts.capturedAt ? ` · captured ${escapeHtml(artifacts.capturedAt)}` : ''}</p>
      ${artifacts.catalogPath ? `<p>Exact catalog: <code>${escapeHtml(artifacts.catalogPath)}</code></p>` : '<p class="muted">Exact catalog is pending startup discovery.</p>'}
      ${artifacts.guidePath ? `<p>Compact guide: <code>${escapeHtml(artifacts.guidePath)}</code></p>` : ''}
      ${promptState === 'stale' ? '<p class="callout">Execution catalog changed after the system prompt froze. Start <code>/new</code> to expose the updated MCP routing catalog to the model.</p>' : ''}
    </section>
    <div class="section-heading" id="skills"><div><h2>Skills</h2><p>Disabled skills disappear from the agent catalog, autocomplete, discovery inventory, and skill loader.</p></div></div><section><div class="skill-toolbar"><input id="skill-filter" type="search" placeholder="Search skills…" aria-label="Search skills"><button class="active" data-skill-filter="all">All</button><button data-skill-filter="enabled">Enabled</button><button data-skill-filter="disabled">Disabled</button></div><div id="skill-list" class="skill-grid">${skillRows || '<p>No skills discovered. Install skills, then reload the session.</p>'}</div><p class="muted">${enabledSkillCount} enabled · ${skills.length - enabledSkillCount} disabled. Changes block or allow loading immediately; start <code>/new</code> to refresh an already-frozen agent prompt.</p></section>
    <div class="section-heading" id="overrides"><div><h2>Workspace overrides</h2><p>Normalized SQLite state; no definitions, schemas, health, or secrets are duplicated.</p></div></div><section><pre>${escapeHtml(JSON.stringify(overrides, null, 2))}</pre></section>
    <div class="section-heading" id="diagnostics"><div><h2>Diagnostics</h2><p>Redacted host and source health for this generated snapshot.</p></div></div><section><div class="row"><span>Host compatibility</span><span class="badge on">Pi 0.84.4</span></div><div class="row"><span>Settings output</span><code>${escapeHtml(path.join(managerDir(cwd), SETTINGS_HTML_FILE))}</code></div><div class="row"><span>Action transport</span><span>loopback · same-origin · token protected</span></div></section>
    </div></div><div id="mcp-toast" class="toast hidden" role="status"></div>
    <script type="module">
      const token = ${JSON.stringify(actionToken)};
      const toast = document.querySelector('#mcp-toast');
      const notice = (message) => { toast.textContent = message; toast.classList.remove('hidden'); setTimeout(() => toast.classList.add('hidden'), 4000); };
      const post = async (action) => {
        const response = await fetch('./__octocode/action', { method:'POST', headers:{'content-type':'application/json','x-octocode-action-token':token}, body:JSON.stringify(action) });
        const value = await response.json().catch(() => ({}));
        if (!response.ok || !value.ok) throw new Error(value.error || 'MCP update failed');
        return value.value;
      };
      document.addEventListener('click', async (event) => {
        const button = event.target.closest('button[data-action]');
        if (!button) return;
        if (button.dataset.action === 'edit') {
          const value = JSON.parse(atob(button.dataset.config));
          const form = document.querySelector('#mcp-editor');
          for (const [key, raw] of Object.entries(value)) {
            const field = form.elements.namedItem(key === 'name' ? 'server' : key);
            if (!field) continue;
            field.value = typeof raw === 'object' ? JSON.stringify(raw, null, 2) : String(raw ?? '');
          }
          syncTransport();
          form.scrollIntoView({ behavior:'smooth' });
          return;
        }
        if (button.dataset.action === 'remove' && !confirm('Remove this MCP server from project configuration?')) return;
        const originalLabel = button.textContent;
        button.disabled = true;
        button.setAttribute('aria-busy', 'true');
        button.textContent = 'Updating…';
        try {
          const skillScope = button.dataset.skill ? button.closest('.skill-card')?.querySelector('[data-skill-scope]')?.value : undefined;
          await post({ action:button.dataset.action, server:button.dataset.server, tool:button.dataset.tool || undefined, skill:button.dataset.skill || undefined, density:button.dataset.density || undefined, level:button.dataset.level || undefined, theme:button.dataset.theme || undefined, source:button.dataset.source || undefined, hash:button.dataset.hash || undefined, expectedRevision:button.dataset.action?.includes('hook') ? ${JSON.stringify(hookSnapshot.revision)} : ${JSON.stringify(settingsRevision)}, scope:skillScope || button.dataset.scope || 'project' });
          location.reload();
        } catch (error) { notice(error.message); button.disabled = false; button.removeAttribute('aria-busy'); button.textContent = originalLabel; }
      });
      let activeFilter = 'all';
      const applyFilter = () => { const query = document.querySelector('#server-filter').value.trim().toLowerCase(); document.querySelectorAll('.server-card').forEach(card => card.classList.toggle('hidden', !card.dataset.search.includes(query) || (activeFilter !== 'all' && card.dataset.origin !== activeFilter))); };
      document.querySelector('#server-filter').addEventListener('input', applyFilter);
      document.querySelectorAll('[data-filter]').forEach(button => button.addEventListener('click', () => { activeFilter = button.dataset.filter; document.querySelectorAll('[data-filter]').forEach(item => item.classList.toggle('active', item === button)); applyFilter(); }));
      const skillFilter = document.querySelector('#skill-filter');
      let activeSkillFilter = 'all';
      const applySkillFilter = () => { const query = skillFilter.value.trim().toLowerCase(); document.querySelectorAll('.skill-card').forEach(card => card.classList.toggle('hidden', !card.dataset.skillSearch.includes(query) || (activeSkillFilter !== 'all' && card.dataset.skillState !== activeSkillFilter))); };
      skillFilter.addEventListener('input', applySkillFilter);
      document.querySelectorAll('[data-skill-filter]').forEach(button => button.addEventListener('click', () => { activeSkillFilter = button.dataset.skillFilter; document.querySelectorAll('[data-skill-filter]').forEach(item => item.classList.toggle('active', item === button)); applySkillFilter(); }));
      const commandFilter = document.querySelector('#command-filter');
      let activeCommandFilter = 'all';
      const applyCommandFilter = () => { const query = commandFilter.value.trim().toLowerCase(); document.querySelectorAll('.command-card').forEach(card => card.classList.toggle('hidden', !card.dataset.commandSearch.includes(query) || (activeCommandFilter !== 'all' && card.dataset.commandSource !== activeCommandFilter))); };
      commandFilter.addEventListener('input', applyCommandFilter);
      document.querySelectorAll('[data-command-filter]').forEach(button => button.addEventListener('click', () => { activeCommandFilter = button.dataset.commandFilter; document.querySelectorAll('[data-command-filter]').forEach(item => item.classList.toggle('active', item === button)); applyCommandFilter(); }));
      const transport = document.querySelector('[name="transport"]');
      const syncTransport = () => document.querySelectorAll('[data-transport-field]').forEach(field => field.classList.toggle('hidden', field.dataset.transportField !== transport.value));
      transport.addEventListener('change', syncTransport);
      syncTransport();
      document.querySelector('#mcp-editor').addEventListener('submit', async (event) => {
        event.preventDefault();
        const form = event.currentTarget;
        const data = new FormData(form);
        const transport = String(data.get('transport'));
        const parseRefs = (name) => { const text = String(data.get(name) || '').trim(); return text ? JSON.parse(text) : undefined; };
        const config = {
          ...(transport === 'http' ? { url:String(data.get('url') || '').trim() } : { command:String(data.get('command') || '').trim(), args:String(data.get('args') || '').split(/\\r?\\n/).map(v => v.trim()).filter(Boolean) }),
          ...(String(data.get('cwd') || '').trim() ? { cwd:String(data.get('cwd')).trim() } : {}),
          timeoutMs:Number(data.get('timeoutMs') || 30000),
          ...(String(data.get('description') || '').trim() ? { description:String(data.get('description')).trim() } : {}),
          ...(parseRefs('envRefs') ? { envRefs:parseRefs('envRefs') } : {}),
          ...(parseRefs('headerRefs') ? { headerRefs:parseRefs('headerRefs') } : {}),
          auth:String(data.get('auth') || 'none'),
        };
        const status = document.querySelector('#mcp-status');
        try { status.textContent = 'Saving and refreshing catalog…'; await post({ action:'add', server:String(data.get('server')), scope:String(data.get('scope')), config }); location.reload(); }
        catch (error) { status.textContent = error.message; }
      });
    </script>`;
  return renderOctocodePage({
    title: 'Configuration',
    eyebrow: 'Octocode · extension control center',
    wide: true,
    bodyHtml,
    footerHtml: 'Everything lives here: live commands, MCP discovery, connections, per-tool enablement, skills, and prompt mode. Close safely and run <code>/configuration</code> whenever you want a fresh snapshot.',
  });
}

export type SettingsSection = 'overview' | 'runtime' | 'appearance' | 'models' | 'hooks' | 'plugins' | 'commands' | 'skills' | 'connections' | 'add-server' | 'sources' | 'agent-context' | 'overrides' | 'diagnostics';

export async function openMcpManager(ctx?: PiContext, piSkills?: SkillInfo[], section?: SettingsSection, commands: readonly PiCommand[] = [], pi?: PiInstance): Promise<{ ok: boolean; url?: string; message?: string }> {
  const cwd = ctx?.cwd ?? process.cwd();
  const dir = managerDir(cwd);
  ensurePrivateDirectory(dir);
  const actionToken = randomBytes(32).toString('hex');
  const settingsFile = path.join(dir, SETTINGS_HTML_FILE);
  const write = async (): Promise<void> => {
    fs.writeFileSync(settingsFile, await renderMcpManagerPage(ctx, actionToken, piSkills, commands, pi), { encoding: 'utf8', mode: PRIVATE_FILE_MODE });
    hardenPrivateFile(settingsFile);
  };
  await write();
  let actionQueue = Promise.resolve();
  const served = await serveDirectory(configurationMountName(cwd), dir, {
    indexFile: SETTINGS_HTML_FILE,
    onAction: (raw) => {
      const pending = actionQueue.then(async () => {
        const action = parseMcpManagerAction(raw);
        await applyMcpManagerAction(action, ctx, pi);
        await write();
        return { updated: true };
      });
      actionQueue = pending.then(() => undefined, () => undefined);
      return pending;
    },
    actionToken,
  });
  if (!served) return { ok: false, message: 'Could not start the local MCP page server.' };
  const settingsUrl = `${served.url}${SETTINGS_HTML_FILE}${section ? `#${section}` : ''}`;
  const opened = await openLocalUrl(settingsUrl, { preference: 'system' });
  return opened.ok ? { ok: true, url: settingsUrl } : { ok: false, url: settingsUrl, message: opened.message };
}


function configurationMountName(cwd: string): string {
  return `configuration-${createHash('sha256').update(path.resolve(cwd)).digest('hex').slice(0, 16)}`;
}

/** Retire browser actions and cached session controls when their session ends. */
export function closeConfiguration(ctx?: PiContext): void {
  const cwd = path.resolve(ctx?.cwd ?? process.cwd());
  unmount(configurationMountName(cwd));
  settingsAdapters.delete(cwd);
  hookDiscovery.delete(cwd);
}
