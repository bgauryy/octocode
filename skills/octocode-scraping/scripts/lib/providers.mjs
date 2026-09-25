import { cleanupCdp, fetchCdp, fetchDirect, fetchScrapingAnt } from './client.mjs';

export const PROVIDERS = {
  scrapingant: { name: 'scrapingant', fetch: fetchScrapingAnt, supportsModes: ['html', 'markdown', 'extended', 'extract'], requiresApiKey: true, apiKeyEnv: 'SCRAPING_ANT' },
  direct: { name: 'direct', fetch: fetchDirect, supportsModes: ['html'], requiresApiKey: false, apiKeyEnv: null },
  cdp: { name: 'cdp', fetch: fetchCdp, cleanup: cleanupCdp, supportsModes: ['html'], requiresApiKey: false, apiKeyEnv: null }
};

export function resolveProvider(name) {
  const provider = PROVIDERS[name];
  if (!provider) throw new Error(`Unknown --provider "${name}". Supported: ${Object.keys(PROVIDERS).join(', ')}, auto`);
  return provider;
}

/**
 * Auto-select a keyless route for html. Hosted is never auto-picked — pass
 * `--provider scrapingant` only after direct/browser evidence is insufficient and the user approved spend.
 * HTML starts with direct HTTP. A browser is an explicit escalation after the
 * direct corpus proves that rendering, interaction, or live network evidence is
 * needed; merely having the Chrome skill installed must not change this route.
 * Non-html modes (markdown / extended / extract) require scrapingant — throw early if unavailable.
 */
export function autoSelectProvider(mode, env) {
  if (mode !== 'html') {
    if (!env.SCRAPING_ANT?.trim()) {
      throw new Error(`--mode ${mode} requires scrapingant; set the SCRAPING_ANT env variable or pass --provider scrapingant explicitly`);
    }
    return 'scrapingant';
  }
  return 'direct';
}
