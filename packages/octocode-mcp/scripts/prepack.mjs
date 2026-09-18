#!/usr/bin/env node
/**
 * Pre-pack guard for octocode-mcp.
 *
 * Runtime and engine addons are distributed through optional platform
 * dependencies owned by @octocodeai/octocode-native. MCP resolves only the
 * native `/runtime` entrypoint; primitive contracts remain independently
 * available from `/engine` without a TypeScript execution fallback.
 */
console.error('✓ octocode-mcp prepack: npm runtime assets are dependency-owned.');
