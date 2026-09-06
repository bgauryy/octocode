import { createHash } from 'node:crypto';
import type { LanguageServerConfig, RustBuildContext } from './types.js';

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

/** Explicit Rust contexts never inherit an executable provider policy. */
export function applyRustBuildContext(
  config: LanguageServerConfig,
  context?: RustBuildContext
): LanguageServerConfig {
  if (!context) return config;
  if (config.languageId !== 'rust')
    throw new Error('rustContext requires a Rust language server.');
  if (context.procMacros && !context.buildScripts) {
    throw new Error(
      'Rust procMacros requires buildScripts:true; rust-analyzer builds procedural macros through Cargo.'
    );
  }
  const options = config.initializationOptions ?? {};
  const cargo = record(options.cargo);
  return {
    ...config,
    initializationOptions: {
      ...options,
      cargo: {
        ...cargo,
        features:
          context.features === 'all'
            ? 'all'
            : [...new Set(context.features ?? [])].sort(),
        noDefaultFeatures: context.noDefaultFeatures ?? false,
        target: context.target ?? null,
        cfgs: [...new Set(context.cfgs ?? [])].sort(),
        buildScripts: {
          ...record(cargo.buildScripts),
          enable: context.buildScripts ?? false,
        },
        targetDir: true,
      },
      procMacro: {
        ...record(options.procMacro),
        enable: context.procMacros ?? false,
      },
      cfg: { ...record(options.cfg), setTest: false },
      checkOnSave: false,
    },
  };
}

/** Hash full initialization settings without exposing environment values. */
export function serverConfigurationFingerprint(
  config: LanguageServerConfig
): string {
  const canonical = JSON.stringify(
    [config.initializationOptions ?? {}, config.env ?? {}],
    (_key, value: unknown) => {
      if (!value || typeof value !== 'object' || Array.isArray(value))
        return value;
      return Object.fromEntries(
        Object.entries(value).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      );
    }
  );
  return createHash('sha256').update(canonical).digest('hex');
}
