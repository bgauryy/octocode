/** `open-browser.mjs --help`. */
export const OPEN_BROWSER_HELP = `Usage: open-browser.mjs [options]

Launch or reuse a Chrome DevTools Protocol session.

Options:
  --port <n>              CDP port (default: 9222)
  --url <url>             Initial URL
  --headless              Use an isolated headless profile
                          (without it the isolated browser is visible; either way its
                          profile is kept per port until cleanup, never your real profile)
  --profile <name>        Use the real browser profile (needs approval; quit the browser;
                          Google Chrome 136+ refuses, Chromium/Chrome for Testing work)
  --chromePath <path>     Explicit Chrome executable
  --windowSize <WxH>      Browser window size
  --enableFeatures <csv>  Chrome feature flags
  --userAgent <ua>        User-Agent override
  --proxyServer <url>     Proxy (also --proxyBypassList, --proxyPacUrl)
  --config <path>         Proxy configuration file
  --cleanup               Stop the isolated session tracked for this port (from any cwd)
  --dry-run               Preview cleanup without stopping Chrome
  -h, --help              Show this help and exit`;
