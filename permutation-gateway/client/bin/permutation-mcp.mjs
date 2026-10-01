#!/usr/bin/env node
// MCP server for Wylls. Configure with environment variables:
//   PS_SERVER   game server   (default http://127.0.0.1:4185)
//   PS_GATEWAY  chain gateway (default http://127.0.0.1:4191)
//   PS_MEMBER   your member id if you already joined (otherwise call join_season)
//   PS_CIV      your nation id (with PS_MEMBER)
//   PS_AGENT_DIR where your wallet/session keys live (default ~/.permutation-agent)
//   PS_POLICY   policy name committed with every decision (default mcp-agent/v1)
import { runMcp } from '../src/mcp.mjs';

runMcp().catch(e => { console.error(e); process.exit(1); });
