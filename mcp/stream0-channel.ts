#!/usr/bin/env bun
/**
 * Stream0 Channel Plugin for Claude Code
 *
 * Bridges Stream0 inbox ↔ Claude Code session.
 * Messages sent to this agent's inbox appear in Claude Code automatically.
 * Claude replies via the reply tool, which sends back through Stream0.
 *
 * Environment variables:
 *   STREAM0_URL       - Stream0 server URL (default: http://localhost:8080)
 *   STREAM0_API_KEY   - API key for authentication
 *   STREAM0_AGENT_ID  - This agent's ID on Stream0
 *
 * Usage:
 *   claude --dangerously-load-development-channels server:stream0-channel
 *
 * .mcp.json:
 *   {
 *     "mcpServers": {
 *       "stream0-channel": {
 *         "command": "bun",
 *         "args": ["./mcp/stream0-channel.ts"],
 *         "env": {
 *           "STREAM0_URL": "https://stream0.dev",
 *           "STREAM0_API_KEY": "sk-xxx",
 *           "STREAM0_AGENT_ID": "cao"
 *         }
 *       }
 *     }
 *   }
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ListToolsRequestSchema,
  CallToolRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";

const STREAM0_URL = process.env.STREAM0_URL || "http://localhost:8080";
const STREAM0_API_KEY = process.env.STREAM0_API_KEY || "";
const AGENT_ID = process.env.STREAM0_AGENT_ID || "";

if (!AGENT_ID) {
  console.error("[stream0-channel] STREAM0_AGENT_ID not set");
  process.exit(1);
}

const headers: Record<string, string> = { "Content-Type": "application/json" };
if (STREAM0_API_KEY) headers["X-API-Key"] = STREAM0_API_KEY;

// --- Stream0 HTTP helpers ---

async function stream0Get(
  path: string,
  params?: Record<string, string>
): Promise<any> {
  const url = new URL(`${STREAM0_URL}${path}`);
  if (params) for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  const resp = await fetch(url.toString(), { headers, signal: AbortSignal.timeout(35000) });
  return resp.json();
}

async function stream0Post(path: string, body?: any): Promise<any> {
  const resp = await fetch(`${STREAM0_URL}${path}`, {
    method: "POST",
    headers,
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(10000),
  });
  return resp.json();
}

// --- MCP Server ---

const mcp = new Server(
  { name: "stream0-channel", version: "0.1.0" },
  {
    capabilities: {
      experimental: { "claude/channel": {} },
      tools: {},
    },
    instructions: `Messages from other agents arrive as <channel source="stream0-channel" thread_id="..." from="..." type="..."> tags.

When you receive a message:
1. Read it and understand what's being asked
2. Do the work
3. Reply using the reply tool with the thread_id and the sender's agent ID
4. Acknowledge the message using the ack tool with the message_id

Message types: request (do work), question (clarification needed), answer (response to your question), done (task complete), failed (task failed), message (general).

Always reply to requests with either done or failed. Never leave a request unanswered.`,
  }
);

// --- Tools: reply and ack ---

mcp.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "reply",
      description:
        "Send a reply back through Stream0 to another agent. Use this after processing a message.",
      inputSchema: {
        type: "object",
        properties: {
          to: { type: "string", description: "The agent ID to reply to (from the channel message)" },
          thread_id: { type: "string", description: "The thread_id from the incoming message" },
          type: {
            type: "string",
            description: "Message type: done, failed, answer, question, or message",
          },
          content: { type: "string", description: "Reply content as JSON string" },
        },
        required: ["to", "thread_id", "type", "content"],
      },
    },
    {
      name: "ack",
      description: "Acknowledge a message after processing it so it won't appear again.",
      inputSchema: {
        type: "object",
        properties: {
          message_id: { type: "string", description: "The message ID to acknowledge" },
        },
        required: ["message_id"],
      },
    },
  ],
}));

mcp.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;

  if (name === "reply") {
    const { to, thread_id, type, content } = args as {
      to: string;
      thread_id: string;
      type: string;
      content: string;
    };

    let contentObj: any;
    try {
      contentObj = JSON.parse(content);
    } catch {
      contentObj = { text: content };
    }

    await stream0Post(`/agents/${to}/inbox`, {
      thread_id,
      from: AGENT_ID,
      type,
      content: contentObj,
    });

    return { content: [{ type: "text", text: `Replied to ${to} (thread: ${thread_id})` }] };
  }

  if (name === "ack") {
    const { message_id } = args as { message_id: string };
    await stream0Post(`/inbox/messages/${message_id}/ack`);
    return { content: [{ type: "text", text: `Acknowledged ${message_id}` }] };
  }

  throw new Error(`Unknown tool: ${name}`);
});

// --- Connect and start polling ---

await mcp.connect(new StdioServerTransport());

// Register agent on Stream0
await stream0Post("/agents", { id: AGENT_ID });
console.error(`[stream0-channel] Registered as ${AGENT_ID}, polling inbox...`);

// Track pushed message IDs to avoid duplicates
const pushed = new Set<string>();

// Poll inbox and push events to Claude Code
async function pollLoop() {
  while (true) {
    try {
      const result = await stream0Get(`/agents/${AGENT_ID}/inbox`, {
        status: "unread",
        timeout: "25",
      });

      const messages = result?.messages || [];
      for (const msg of messages) {
        if (pushed.has(msg.id)) continue;
        pushed.add(msg.id);

        console.error(
          `[stream0-channel] Pushing [${msg.type}] from ${msg.from} (thread: ${msg.thread_id})`
        );

        await mcp.notification({
          method: "notifications/claude/channel",
          params: {
            content: JSON.stringify(msg.content || {}),
            meta: {
              message_id: msg.id,
              thread_id: msg.thread_id,
              from: msg.from,
              type: msg.type,
            },
          },
        });
      }
    } catch (e: any) {
      if (e?.name !== "TimeoutError") {
        console.error(`[stream0-channel] Error: ${e?.message || e}`);
        await Bun.sleep(3000);
      }
    }
  }
}

pollLoop();
