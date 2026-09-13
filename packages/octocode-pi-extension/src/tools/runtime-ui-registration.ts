import { getAssetPaths } from '../assets.js';
import { renderBannerWithTagline, type BannerSessionInfo, type BannerTheme } from '../branding/banner.js';
import { readOwnVersion } from '../package-metadata.js';
import { OCTOCODE_BANNER_ENTRY_TYPE, updateOctocodeMetricsUi } from '../extension-ui.js';
import { registerRuntimeInspectors } from '../tui/runtime-inspector.js';
import type { NotifyFn, PiInstance } from '../types.js';
import { registerAwarenessRuntime } from './awareness-runtime.js';
import { registerCompactionHooks } from './compaction-hooks.js';
import { registerCompactionPolicyGuidance } from './compaction-policy-guidance.js';
import { buildRecoveryCard, registerOctocodeMessageRenderers } from './custom-messages.js';
import { registerLifecycleUi } from './lifecycle-ui.js';
import { REHYDRATION_RECEIPT_ENTRY_TYPE } from './rehydration-orchestrator.js';
import { makeComponentRenderer } from './render-helpers.js';

interface RuntimeUiRegistrationArgs {
  pi: PiInstance;
  notify: NotifyFn;
}

/** Own the host UI registrations that must be installed as one runtime phase. */
export function registerRuntimeUiPhase({ pi, notify }: RuntimeUiRegistrationArgs): void {
  registerLifecycleUi(pi, updateOctocodeMetricsUi);
  registerRuntimeInspectors(pi);
  registerCompactionHooks(pi, notify);
  registerCompactionPolicyGuidance(pi, notify);
  registerAwarenessRuntime(pi, { refreshUi: updateOctocodeMetricsUi });

  // Branded conversation cards (compaction checkpoints / awareness peer events)
  // must be registered before compaction-hooks emits the first card.
  registerOctocodeMessageRenderers(pi);
  pi.registerEntryRenderer?.(REHYDRATION_RECEIPT_ENTRY_TYPE, (entry, options, theme) =>
    makeComponentRenderer(
      (_props, { width }) => buildRecoveryCard(entry.data, options?.expanded === true, theme, width),
      undefined,
    ),
  );

  // Fresh-session banner card: a durable TUI-only transcript entry (never in
  // LLM context) that re-renders from its immutable session-info snapshot.
  pi.registerEntryRenderer?.(OCTOCODE_BANNER_ENTRY_TYPE, (entry, _options, theme) => {
    const data = entry as { model?: string; provider?: string; thinking?: string } | undefined;
    const sessionInfo: BannerSessionInfo | undefined =
      data?.model || data?.provider || data?.thinking
        ? { model: data.model, provider: data.provider, thinking: data.thinking }
        : undefined;
    return makeComponentRenderer(
      (_props, { width }) =>
        renderBannerWithTagline(
          theme as BannerTheme,
          width,
          readOwnVersion(getAssetPaths().baseDir),
          sessionInfo,
        ),
      undefined,
    );
  });
}
