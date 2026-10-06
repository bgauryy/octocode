/** Per-page setup the session applies to every page it drives (the first and each tab it switches to). */

/** The one CDP call page setup needs, so this module does not depend on the connection code. */
interface CdpSender {
  send<T = Record<string, unknown>>(method: string, params?: Record<string, unknown>, signal?: AbortSignal): Promise<T>;
}

/** Saves the page's downloads to `dir` and reports their progress as events. */
export async function applyDownloads(page: CdpSender, dir: string, signal?: AbortSignal): Promise<void> {
  // Browser.* is the current spelling; older Chrome builds only answer the page-level one.
  await page.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: dir, eventsEnabled: true }, signal).catch(() => page.send('Page.setDownloadBehavior', { behavior: 'allow', downloadPath: dir }, signal));
}

/**
 * Pins the page's language (`OCTOCODE_BROWSER_LOCALE`, e.g. `en-US`: navigator.language, Intl formatting and the
 * Accept-Language header) and time zone (`OCTOCODE_BROWSER_TIMEZONE`, e.g. `America/New_York`). Sites that price by
 * IP address still answer for where the machine is; only a proxy changes that.
 */
export async function applyLocale(page: CdpSender, env: NodeJS.ProcessEnv): Promise<void> {
  const locale = env['OCTOCODE_BROWSER_LOCALE']?.trim();
  const timezoneId = env['OCTOCODE_BROWSER_TIMEZONE']?.trim();
  if (locale) {
    const { result } = await page.send<{ result: { value?: unknown } }>('Runtime.evaluate', { expression: 'navigator.userAgent', returnByValue: true });
    const language = locale.split('-')[0];
    await page.send('Emulation.setLocaleOverride', { locale });
    await page.send('Emulation.setUserAgentOverride', { userAgent: String(result.value ?? ''), acceptLanguage: language && language !== locale ? `${locale},${language};q=0.9` : locale });
  }
  if (timezoneId) await page.send('Emulation.setTimezoneOverride', { timezoneId });
}
