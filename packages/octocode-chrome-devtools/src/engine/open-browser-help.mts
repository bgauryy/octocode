/** `open-browser.mjs --help`. */
export const OPEN_BROWSER_HELP = `Usage: open-browser.mjs [options]

Launch or reuse a Chrome DevTools Protocol session.

Options:
  --port <n>              CDP port (default: 9222)
  --url <url>             Initial URL
  --urls <a,b,...>        One tab per URL (a group); replies tabs: [{url, targetId, status}]
                          and the group name; select a tab with --group <name> [--group-index <n>]
  --background            With --urls: open the tabs without focusing them
  --new-window            With --urls: open each tab in a new window
  --isolated              With --urls: one separate browser context (own cookies and storage);
                          cleanup disposes it
  --attach                Connect only to a Chrome already listening on --port (required);
                          never launches and never scans other ports
  --allow-origin <csv>    Origins plans and new tabs may navigate to on this port (a mistake
                          guard, not a security boundary); also updates a reused browser
  --no-sandbox            Launch Chrome without its sandbox (containers, kernels without user
                          namespaces); applied automatically for root on Linux
  --headless              Use an isolated headless profile
                          (without it the isolated browser is visible; either way its
                          profile is kept per port until cleanup, never your real profile)
  --profile <name>        Use the real browser profile (needs approval; quit the browser;
                          Google Chrome 136+ refuses, Chromium/Chrome for Testing work)
  --chromePath <path>     Explicit Chrome executable
  --windowSize <WxH>      Browser window size
  --enableFeatures <csv>  More Chrome feature flags (a fresh launch always enables WebMCP)
  --userAgent <ua>        User-Agent override
  --proxyServer <url>     Proxy (also --proxyBypassList, --proxyPacUrl)
  --config <path>         Proxy configuration file
  --cleanup               Stop the isolated session tracked for this port (from any cwd)
  --dry-run               Preview cleanup without stopping Chrome
  -h, --help              Show this help and exit`;
