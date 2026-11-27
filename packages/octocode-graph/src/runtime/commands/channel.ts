#!/usr/bin/env node
// octocode-graph channel: the MCP server (mcp-server.ts) with the live viewer's tools, as a Claude Code channel.
import { serve } from '../mcp-server.ts';

serve('channel');
