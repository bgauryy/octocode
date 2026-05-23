# Troubleshooting Guide

> Common issues and solutions for Octocode MCP.

## Quick Diagnostics

Run this command to check your environment:

```bash
npx node-doctor info
```

This will diagnose common Node.js issues and let your AI agent help resolve them automatically.

---

## Octocode Home Layout

Octocode stores local state under `${OCTOCODE_HOME:-~/.octocode}`.

```
~/.octocode/
├── .octocoderc           # Global config
├── credentials.json      # Encrypted credentials
├── .key                  # Encryption key (local-only)
├── session.json          # Session telemetry state
├── repos/                # Cloned repository cache
├── logs/                 # Runtime logs
├── config.json           # CLI config
└── lsp-servers.json      # User-level LSP server config
```

If you set `OCTOCODE_HOME`, all of these paths move under that directory.

---

## Table of Contents

- [1. npm Registry Issues](#1-npm-registry-issues)
- [2. Node.js Version and Process Issues](#2-nodejs-version-and-process-issues)
- [3. Authentication Issues](#3-authentication-issues)
- [4. MCP Server Connection Issues](#4-mcp-server-connection-issues)
- [5. Still Having Issues?](#5-still-having-issues)

---

## 1. npm Registry Issues

### Symptom: `npm ERR! 404 Not Found` or slow/hanging installs

If you're behind a VPN or using a private npm registry, the default npm registry may not be accessible.

### Check Your Current Registry

```bash
npm config get registry
```

**Expected output:** `https://registry.npmjs.org/`

### Solutions

#### A. Reset to Default Registry

```bash
npm config set registry https://registry.npmjs.org/
```

#### B. Using a Private Registry

If your organization uses a private registry (e.g., Artifactory, Verdaccio, GitHub Packages):

```bash
# Set your private registry
npm config set registry https://your-private-registry.company.com/

# Or use scoped registries for specific packages
npm config set @your-org:registry https://npm.pkg.github.com/
```

#### C. VPN-Related Issues

If you're on a corporate VPN:

1. **Check if registry is blocked:**
   ```bash
   curl -I https://registry.npmjs.org/
   ```

2. **Try with proxy settings:**
   ```bash
   npm config set proxy http://your-proxy:8080
   npm config set https-proxy http://your-proxy:8080
   ```

3. **Or bypass SSL verification (not recommended for production):**
   ```bash
   npm config set strict-ssl false
   ```

#### D. Clear npm Cache

Sometimes cache corruption causes issues:

```bash
npm cache clean --force
```

---

## 2. Node.js Version and Process Issues

### Recommended: Let Your AI Agent Handle It

Run the diagnostic tool and share the output with your AI agent:

```bash
npx node-doctor info
```

**Your AI agent will automatically:**
- Diagnose version compatibility issues
- Identify PATH and environment problems
- Detect conflicting Node.js installations
- Suggest and apply the correct fixes

Simply paste the output into your conversation and let the agent resolve it.

### What the Diagnostic Checks

| Check | Description |
|-------|-------------|
| Node.js version | Minimum **20.0.0** required |
| npm version | Minimum **10.0.0** required |
| PATH configuration | Ensures Node is accessible |
| Multiple installations | Detects nvm, homebrew, system conflicts |
| Permission issues | Identifies npm/node_modules ownership problems |

### Manual Verification (if needed)

```bash
node --version  # Should be >= 20.0.0
npm --version   # Should be >= 10.0.0
which node      # Check which Node is being used
which npm       # Should match Node installation
```

### MCP Config: Ensure Node is Found

If MCP server can't find Node, add PATH to your config:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["octocode-mcp@latest"],
      "env": {
        "PATH": "/usr/local/bin:/opt/homebrew/bin:$PATH"
      }
    }
  }
}
```

Or use an absolute path:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "/usr/local/bin/npx",
      "args": ["octocode-mcp@latest"]
    }
  }
}
```

---

## 3. Authentication Issues

For detailed authentication setup and troubleshooting, see the [Authentication Setup Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md).

**Quick tips not covered there:**
- **401/bad credentials:** Verify token: `curl -H "Authorization: token $GITHUB_TOKEN" https://api.github.com/user`. Ensure scopes include `repo`, `read:user`, `read:org`.
- **GitLab:** Test connection: `curl --header "PRIVATE-TOKEN: $GITLAB_TOKEN" https://gitlab.com/api/v4/user`

---

## 4. MCP Server Connection Issues

### Server Not Starting

#### Check if server is running

```bash
# For Research Skill
curl http://localhost:1987/health

# Check process
ps aux | grep octocode
```

#### Common Fixes

1. **Port already in use:**
   ```bash
   # Find process using port
   lsof -i :1987
   
   # Kill it
   kill -9 <PID>
   ```

2. **Restart MCP client:**
   - **Cursor:** Cmd/Ctrl + Shift + P → "Reload Window"
   - **Claude Desktop:** Quit and relaunch
   - **VS Code:** Restart extension host

3. **Check MCP configuration:**
   ```json
   {
     "mcpServers": {
       "octocode": {
         "command": "npx",
         "args": ["octocode-mcp@latest"]
       }
     }
   }
   ```

---

## 5. Still Having Issues?

If your issue persists after trying the solutions above:

1. **Search existing issues:** [github.com/bgauryy/octocode-mcp/issues](https://github.com/bgauryy/octocode-mcp/issues)
2. **Report a new issue:** Include diagnostic output (`npx node-doctor info`), your environment details, and steps to reproduce

We actively monitor and respond to issues!

---

## See Also

- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — All env vars and `.octocoderc` options
- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) — GitHub/GitLab auth guide
- [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) — Build commands, testing, code standards
