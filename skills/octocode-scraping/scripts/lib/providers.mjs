import { existsSync } from 'node:fs';
import { CHROME_DEVTOOLS_DIR, cleanupCdp, fetchCdp, fetchDirect, fetchScrapingAnt } from './client.mjs';

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
 * Auto-select the best available provider based on environment and installed skills.
 * Priority: scrapingant (SCRAPING_ANT set) → cdp (octocode-chrome-devtools installed) → direct.
 * Non-html modes (markdown / extended / extract) require scrapingant — throw early if unavailable.
 */
export function autoSelectProvider(mode, env) {
  if (mode !== 'html') {
    if (!env.SCRAPING_ANT?.trim()) {
      throw new Error(`--mode ${mode} requires scrapingant; set the SCRAPING_ANT env variable or pass --provider scrapingant explicitly`);
    }
    return 'scrapingant';
  }
  if (env.SCRAPING_ANT?.trim()) return 'scrapingant';
  if (existsSync(CHROME_DEVTOOLS_DIR)) return 'cdp';
  return 'direct';
}
