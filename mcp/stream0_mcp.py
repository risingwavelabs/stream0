"""
Stream0 MCP Server — gives Claude Code agents native access to Stream0.

Tools:
  - check_inbox: Check for unread messages
  - send_message: Send a message to another agent
  - ack_message: Acknowledge a processed message
  - list_agents: See who's registered and online
  - get_history: View full conversation thread

Usage in .mcp.json:
{
  "stream0": {
    "command": "uv",
    "args": ["run", "stream0_mcp.py"],
    "env": {
      "STREAM0_URL": "https://stream0.dev",
      "STREAM0_API_KEY": "sk-xxx",
      "STREAM0_AGENT_ID": "my-agent"
    }
  }
}
"""

import os
import sys
import logging
import httpx
from mcp.server.fastmcp import FastMCP

logging.basicConfig(level=logging.INFO, stream=sys.stderr)

# Config from environment
STREAM0_URL = os.environ.get("STREAM0_URL", "http://localhost:8080")
STREAM0_API_KEY = os.environ.get("STREAM0_API_KEY", "")
STREAM0_AGENT_ID = os.environ.get("STREAM0_AGENT_ID", "")

HEADERS = {"Content-Type": "application/json"}
if STREAM0_API_KEY:
    HEADERS["X-API-Key"] = STREAM0_API_KEY

mcp = FastMCP("stream0")


def _url(path: str) -> str:
    return f"{STREAM0_URL.rstrip('/')}{path}"


async def _get(path: str, params: dict = None) -> dict:
    async with httpx.AsyncClient() as client:
        resp = await client.get(_url(path), headers=HEADERS, params=params, timeout=35)
        resp.raise_for_status()
        return resp.json()


async def _post(path: str, json: dict = None) -> dict:
    async with httpx.AsyncClient() as client:
        resp = await client.post(_url(path), headers=HEADERS, json=json, timeout=10)
        resp.raise_for_status()
        return resp.json()


async def _delete(path: str) -> dict:
    async with httpx.AsyncClient() as client:
        resp = await client.delete(_url(path), headers=HEADERS, timeout=10)
        resp.raise_for_status()
        return resp.json()


# --- Tools ---


@mcp.tool()
async def register(agent_id: str = "", aliases: list[str] = None) -> str:
    """Register this agent with Stream0. Call this at the start of every session.

    Args:
        agent_id: Your agent ID. Defaults to STREAM0_AGENT_ID env var.
        aliases: Optional list of alternate names for this agent.
    """
    aid = agent_id or STREAM0_AGENT_ID
    if not aid:
        return "Error: No agent_id provided and STREAM0_AGENT_ID not set."

    body = {"id": aid}
    if aliases:
        body["aliases"] = aliases

    result = await _post("/agents", body)
    return f"Registered as '{aid}'. Inbox is ready."


@mcp.tool()
async def check_inbox(
    thread_id: str = "",
    timeout: int = 0,
) -> str:
    """Check your inbox for unread messages. Call this at the start of every session
    and whenever you're waiting for a response.

    Args:
        thread_id: Filter by thread (optional). Leave empty for all unread messages.
        timeout: Long-poll timeout in seconds (0 for immediate, up to 30).
    """
    aid = STREAM0_AGENT_ID
    if not aid:
        return "Error: STREAM0_AGENT_ID not set."

    params = {"status": "unread"}
    if thread_id:
        params["thread_id"] = thread_id
    if timeout > 0:
        params["timeout"] = str(timeout)

    result = await _get(f"/agents/{aid}/inbox", params)
    messages = result.get("messages", [])

    if not messages:
        return "No unread messages."

    lines = [f"You have {len(messages)} unread message(s):\n"]
    for msg in messages:
        lines.append(f"--- Message {msg['id']} ---")
        lines.append(f"  Thread: {msg.get('thread_id', 'N/A')}")
        lines.append(f"  From: {msg.get('from', 'unknown')}")
        lines.append(f"  Type: {msg.get('type', 'unknown')}")
        content = msg.get("content")
        if content:
            lines.append(f"  Content: {content}")
        lines.append("")

    return "\n".join(lines)


@mcp.tool()
async def send_message(
    to: str,
    thread_id: str,
    type: str,
    content: str = "",
) -> str:
    """Send a message to another agent's inbox.

    Args:
        to: The recipient agent ID.
        thread_id: Thread/conversation identifier (groups related messages).
        type: Message type — one of: request, question, answer, done, failed, message.
        content: Message content as a JSON string (e.g. '{"instruction": "review this PR"}').
    """
    aid = STREAM0_AGENT_ID
    if not aid:
        return "Error: STREAM0_AGENT_ID not set."

    import json
    try:
        content_obj = json.loads(content) if content else None
    except json.JSONDecodeError:
        content_obj = {"text": content}

    body = {
        "thread_id": thread_id,
        "from": aid,
        "type": type,
    }
    if content_obj is not None:
        body["content"] = content_obj

    result = await _post(f"/agents/{to}/inbox", body)
    return f"Message sent to '{to}' (thread: {thread_id}, type: {type}). ID: {result.get('message_id', 'unknown')}"


@mcp.tool()
async def ack_message(message_id: str) -> str:
    """Mark a message as read/processed. Call this after you've handled a message.

    Args:
        message_id: The message ID to acknowledge (from check_inbox results).
    """
    result = await _post(f"/inbox/messages/{message_id}/ack")
    return f"Message {message_id} acknowledged."


@mcp.tool()
async def list_agents() -> str:
    """List all registered agents and their status (online/offline)."""
    result = await _get("/agents")
    agents = result.get("agents", [])

    if not agents:
        return "No agents registered."

    lines = [f"{len(agents)} agent(s) registered:\n"]
    for agent in agents:
        status = "online" if agent.get("last_seen") else "never seen"
        if agent.get("last_seen"):
            status = f"last seen {agent['last_seen']}"
        aliases = agent.get("aliases", [])
        alias_str = f" (aliases: {', '.join(aliases)})" if aliases else ""
        lines.append(f"  - {agent['id']}{alias_str} [{status}]")

    return "\n".join(lines)


@mcp.tool()
async def get_history(thread_id: str) -> str:
    """View the full conversation history for a thread.

    Args:
        thread_id: The thread/conversation ID to retrieve.
    """
    result = await _get(f"/threads/{thread_id}/messages")
    messages = result.get("messages", [])

    if not messages:
        return f"No messages found for thread '{thread_id}'."

    lines = [f"Thread '{thread_id}' — {len(messages)} message(s):\n"]
    for msg in messages:
        lines.append(f"  [{msg.get('type', '?')}] {msg.get('from', '?')} → {msg.get('to', '?')}")
        content = msg.get("content")
        if content:
            lines.append(f"    {content}")

    return "\n".join(lines)


if __name__ == "__main__":
    mcp.run()
