#!/usr/bin/env node
/**
 * Stream0 Channel - MCP server for Claude Code
 *
 * Bridges Stream0 inbox <-> Claude Code session.
 * Install: npx stream0-channel
 *
 * Environment variables:
 *   STREAM0_URL       - Stream0 server URL (default: http://localhost:8080)
 *   STREAM0_API_KEY   - API key for authentication
 *   STREAM0_AGENT_ID  - This agent's ID on Stream0
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

const headers = { "Content-Type": "application/json" };
if (STREAM0_API_KEY) headers["X-API-Key"] = STREAM0_API_KEY;

// --- Stream0 HTTP helpers ---

async function stream0Get(path, params) {
  const url = new URL(`${STREAM0_URL}${path}`);
  if (params) for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  const resp = await fetch(url.toString(), { headers, signal: AbortSignal.timeout(35000) });
  return resp.json();
}

async function stream0Post(path, body) {
  const resp = await fetch(`${STREAM0_URL}${path}`, {
    method: "POST",
    headers,
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(10000),
  });
  return resp.json();
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// --- MCP Server ---

const mcp = new Server(
  { name: "stream0-channel", version: "0.2.0" },
  {
    capabilities: {
      experimental: { "claude/channel": {} },
      tools: {},
    },
    instructions: `You are connected to Stream0, an agent collaboration network.

## Proactive collaboration

When the user asks you to collaborate with, delegate to, or consult other agents:
1. Use the **discover** tool to see which agents are available and what they do
2. Use the **delegate** tool to send a task and wait for the result
3. Present the result to the user

Examples of user requests that should trigger collaboration:
- "find someone to review my code"
- "ask the reviewer to look at this"
- "get feedback from other agents"
- "discuss this with the team"

## Responding to incoming messages

Messages from other agents arrive as <channel source="stream0-channel" thread_id="..." from="..." type="..."> tags.

When you receive a message:
1. Read it and understand what's being asked
2. Do the work
3. Reply using the reply tool with the thread_id and the sender's agent ID
4. Acknowledge the message using the ack tool with the message_id

Message types: request (do work), question (clarification needed), answer (response to your question), done (task complete), failed (task failed), message (general).

Always reply to requests with either done or failed. Never leave a request unanswered.`,
  }
);

// --- Tools ---

mcp.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "discover",
      description:
        "List all available agents on Stream0 with their descriptions. Use this to find agents that can help with a task.",
      inputSchema: {
        type: "object",
        properties: {},
      },
    },
    {
      name: "delegate",
      description:
        "Send a task to another agent and wait for their response. Handles the full lifecycle: sends the request, waits for the result, and returns the response. Use this when the user asks you to collaborate with or get help from another agent.",
      inputSchema: {
        type: "object",
        properties: {
          to: {
            type: "string",
            description: "The agent ID to send the task to",
          },
          task: {
            type: "string",
            description: "Description of what you need the agent to do",
          },
          context: {
            type: "string",
            description:
              "Additional context like code diffs, file contents, or other details the agent needs",
          },
          timeout: {
            type: "number",
            description: "Max seconds to wait for a response (default: 120, max: 300)",
          },
        },
        required: ["to", "task"],
      },
    },
    {
      name: "reply",
      description:
        "Send a reply back through Stream0 to another agent. Use this after processing an incoming message.",
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

  // --- discover ---
  if (name === "discover") {
    const result = await stream0Get("/agents");
    const agents = (result?.agents || [])
      .filter((a) => a.id !== AGENT_ID)
      .map((a) => ({
        id: a.id,
        description: a.description || "(no description)",
        aliases: a.aliases || [],
        online: a.last_seen
          ? Date.now() - new Date(a.last_seen).getTime() < 5 * 60 * 1000
          : false,
      }));

    if (agents.length === 0) {
      return {
        content: [
          {
            type: "text",
            text: "No other agents are registered on Stream0. Start a worker agent first.",
          },
        ],
      };
    }

    const lines = agents.map(
      (a) =>
        `- **${a.id}**${a.online ? " (online)" : " (offline)"}: ${a.description}${a.aliases.length ? ` [aliases: ${a.aliases.join(", ")}]` : ""}`
    );

    return {
      content: [
        { type: "text", text: `Available agents:\n\n${lines.join("\n")}` },
      ],
    };
  }

  // --- delegate ---
  if (name === "delegate") {
    const { to, task, context, timeout: userTimeout } = args;

    const timeoutSec = Math.min(Math.max(userTimeout || 120, 10), 300);
    const threadId = `delegate-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

    const content = { task };
    if (context) content.context = context;

    await stream0Post(`/agents/${to}/inbox`, {
      thread_id: threadId,
      from: AGENT_ID,
      type: "request",
      content,
    });

    console.error(
      `[stream0-channel] Delegated to ${to} (thread: ${threadId}), waiting up to ${timeoutSec}s...`
    );

    const deadline = Date.now() + timeoutSec * 1000;

    while (Date.now() < deadline) {
      const pollTimeout = Math.min(25, Math.ceil((deadline - Date.now()) / 1000));
      if (pollTimeout <= 0) break;

      const result = await stream0Get(`/agents/${AGENT_ID}/inbox`, {
        status: "unread",
        thread_id: threadId,
        timeout: String(pollTimeout),
      });

      const messages = result?.messages || [];
      for (const msg of messages) {
        await stream0Post(`/inbox/messages/${msg.id}/ack`);

        if (msg.type === "done") {
          const responseText =
            typeof msg.content === "string"
              ? msg.content
              : JSON.stringify(msg.content, null, 2);

          return {
            content: [
              {
                type: "text",
                text: `**${to}** completed the task (thread: ${threadId}):\n\n${responseText}`,
              },
            ],
          };
        }

        if (msg.type === "failed") {
          const errorText =
            typeof msg.content === "string"
              ? msg.content
              : JSON.stringify(msg.content, null, 2);

          return {
            content: [
              {
                type: "text",
                text: `**${to}** failed (thread: ${threadId}):\n\n${errorText}`,
              },
            ],
          };
        }

        if (msg.type === "question") {
          const questionText =
            typeof msg.content === "string"
              ? msg.content
              : JSON.stringify(msg.content, null, 2);

          return {
            content: [
              {
                type: "text",
                text: `**${to}** has a question (thread: ${threadId}):\n\n${questionText}\n\nUse the reply tool to answer: reply to="${to}" thread_id="${threadId}" type="answer"`,
              },
            ],
          };
        }

        console.error(
          `[stream0-channel] Received [${msg.type}] from ${msg.from} on delegate thread, continuing to wait...`
        );
      }
    }

    return {
      content: [
        {
          type: "text",
          text: `Timed out waiting for **${to}** to respond after ${timeoutSec}s (thread: ${threadId}). The agent may still be working.`,
        },
      ],
    };
  }

  // --- reply ---
  if (name === "reply") {
    const { to, thread_id, type, content } = args;

    let contentObj;
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

  // --- ack ---
  if (name === "ack") {
    const { message_id } = args;
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

const pushed = new Set();

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
    } catch (e) {
      if (e?.name !== "TimeoutError") {
        console.error(`[stream0-channel] Error: ${e?.message || e}`);
        await sleep(3000);
      }
    }
  }
}

pollLoop();
