import type {
  SettingsMutation,
  SettingsMutationResult,
  SettingsService,
  SettingsSnapshot,
} from '@octocodeai/agent-core';

/**
 * Pi consumes canonical settings state; it does not parse or persist settings
 * itself. Host presentation/model refreshes subscribe only to redacted,
 * successful mutation results.
 */
export class PiSettingsAdapter {
  readonly #listeners = new Set<(result: Extract<SettingsMutationResult, { ok: true }>) => void | Promise<void>>();

  constructor(private readonly service: SettingsService) {}

  snapshot(): SettingsSnapshot {
    return this.service.snapshot();
  }

  subscribe(listener: (result: Extract<SettingsMutationResult, { ok: true }>) => void | Promise<void>): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  async mutate(mutation: SettingsMutation): Promise<SettingsMutationResult> {
    const result = await this.service.mutate(mutation);
    if (!result.ok) return result;
    for (const listener of this.#listeners) await listener(result);
    return result;
  }
}
