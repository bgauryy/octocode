import {
  AWARENESS_CONFIG_QUESTIONS,
  AWARENESS_CONFIG_VERSION,
  AwarenessConfig,
  AwarenessFeatureConfig,
  loadAwarenessConfig,
  writeAwarenessConfig,
} from '../src/awareness-config.js';
import { ParsedArgs } from './cli-model.js';
import { EmitOptions, emit } from './cli-routing.js';

const FEATURE_FLAGS: Record<keyof AwarenessFeatureConfig, string> = {
  hooks: 'hooks',
  notifications: 'notifications',
  verificationGate: 'verification_gate',
  sessionCapture: 'session_capture',
  maintenanceReminders: 'maintenance_reminders',
};

function booleanAnswer(args: ParsedArgs, key: string): boolean | undefined {
  const value = args[key];
  if (value === undefined) return undefined;
  if (value === true || value === 'true') return true;
  if (value === false || value === 'false') return false;
  throw new Error(`--${key.replace(/_/g, '-')} must be true or false`);
}

function supportContract() {
  return {
    configurable: Object.values(FEATURE_FLAGS).map((flag) => `features.${flag.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())}`),
    fixed: [
      'Hook installation always requires separate, just-in-time user approval.',
      'Install/check/remove supports claude, codex, and cursor; Pi native events are not installed or controlled here.',
      'When hooks are enabled, mutation lock enforcement and presence declaration stay enabled together.',
      'Explicit CLI commands, evidence requirements, and database integrity checks are not disabled by this file.',
      'Awareness defaults to $OCTOCODE_HOME/awareness/awareness.sqlite3; explicit repo scope uses <workspace>/.octocode/awareness.sqlite3.',
    ],
  };
}

export function cmdAwarenessConfig(args: ParsedArgs, opts: EmitOptions): number {
  const action = String(args.action ?? 'show');
  if (!['show', 'init', 'validate'].includes(action)) {
    return emit({ ok: false, error: 'config action must be show, init, or validate' }, 1, opts);
  }

  if (action === 'show') {
    try {
      const loaded = loadAwarenessConfig();
      return emit({
        ok: true,
        action,
        path: loaded.path,
        exists: loaded.exists,
        source: loaded.exists ? 'file' : 'defaults',
        automation_active: loaded.exists && loaded.config.features.hooks,
        config: loaded.config,
        ...(loaded.exists ? {} : { requires_user_answers: true, questions: AWARENESS_CONFIG_QUESTIONS }),
        support: supportContract(),
      }, 0, opts);
    } catch (error) {
      return emit({ ok: false, action, error: (error as Error).message, support: supportContract() }, 1, opts);
    }
  }

  if (action === 'validate') {
    try {
      const loaded = loadAwarenessConfig();
      if (!loaded.exists) {
        return emit({
          ok: false,
          action,
          path: loaded.path,
          error: 'awareness configuration is missing; ask all onboarding questions before creating it',
          requires_user_answers: true,
          questions: AWARENESS_CONFIG_QUESTIONS,
        }, 1, opts);
      }
      return emit({ ok: true, action, path: loaded.path, config: loaded.config }, 0, opts);
    } catch (error) {
      return emit({ ok: false, action, error: (error as Error).message }, 1, opts);
    }
  }

  try {
    const existing = loadAwarenessConfig();
    if (existing.exists) {
      return emit({ ok: false, action, path: existing.path, error: 'awareness configuration already exists; edit it explicitly, then run config validate' }, 1, opts);
    }
    const answers = Object.fromEntries(
      Object.entries(FEATURE_FLAGS).map(([feature, flag]) => [feature, booleanAnswer(args, flag)]),
    ) as Record<keyof AwarenessFeatureConfig, boolean | undefined>;
    const missing = Object.entries(answers).filter(([, value]) => value === undefined).map(([key]) => key);
    if (missing.length > 0) {
      return emit({
        ok: false,
        action,
        path: existing.path,
        error: `all onboarding answers are required; missing: ${missing.join(', ')}`,
        requires_user_answers: true,
        questions: AWARENESS_CONFIG_QUESTIONS,
      }, 1, opts);
    }
    const config: AwarenessConfig = {
      version: AWARENESS_CONFIG_VERSION,
      features: answers as AwarenessFeatureConfig,
    };
    const path = writeAwarenessConfig(config);
    return emit({ ok: true, action, created: true, path, config }, 0, opts);
  } catch (error) {
    return emit({ ok: false, action, error: (error as Error).message }, 1, opts);
  }
}
