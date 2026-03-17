import requests as _requests

from stream0.exceptions import (
    AuthenticationError,
    NotFoundError,
    ServerError,
    Stream0Error,
    TimeoutError,
)


class Stream0Client:
    """HTTP client for stream0 event streaming service.

    Every method maps directly to 1-3 HTTP calls. No magic, no hidden state.
    """

    def __init__(self, base_url, api_key=None, timeout=30):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._session = _requests.Session()
        self._session.headers["Content-Type"] = "application/json"
        if api_key:
            self._session.headers["X-API-Key"] = api_key

    def _url(self, path):
        return f"{self.base_url}{path}"

    def _handle_response(self, resp):
        if resp.status_code == 401:
            raise AuthenticationError(
                resp.json().get("error", "authentication failed"),
                status_code=401,
                response=resp,
            )
        if resp.status_code == 404:
            raise NotFoundError(
                resp.json().get("error", "not found"),
                status_code=404,
                response=resp,
            )
        if resp.status_code == 504:
            raise TimeoutError(
                resp.json().get("error", "request timed out"),
                status_code=504,
                response=resp,
            )
        if resp.status_code >= 500:
            raise ServerError(
                resp.json().get("error", "server error"),
                status_code=resp.status_code,
                response=resp,
            )
        if resp.status_code >= 400:
            raise Stream0Error(
                resp.json().get("error", "request failed"),
                status_code=resp.status_code,
                response=resp,
            )
        return resp.json()

    # --- Health ---

    def health(self):
        """Check server health. Returns dict with status and version."""
        resp = self._session.get(self._url("/health"), timeout=self.timeout)
        return self._handle_response(resp)

    # --- Topics ---

    def create_topic(self, name, retention_days=7):
        """Create a topic. Returns topic dict. Idempotent."""
        resp = self._session.post(
            self._url("/topics"),
            json={"name": name, "retention_days": retention_days},
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def list_topics(self):
        """List all topics. Returns list of topic dicts."""
        resp = self._session.get(self._url("/topics"), timeout=self.timeout)
        return self._handle_response(resp)

    def get_topic(self, name):
        """Get topic details by name. Returns topic dict."""
        resp = self._session.get(
            self._url(f"/topics/{name}"), timeout=self.timeout
        )
        return self._handle_response(resp)

    # --- Messages ---

    def publish(self, topic, payload, headers=None):
        """Publish a message to a topic.

        Args:
            topic: Topic name.
            payload: Message payload (dict).
            headers: Optional message headers (dict of str->str).

        Returns:
            Dict with message_id, offset, timestamp.
        """
        body = {"payload": payload}
        if headers:
            body["headers"] = headers
        resp = self._session.post(
            self._url(f"/topics/{topic}/messages"),
            json=body,
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def consume(self, topic, group, max_messages=10, timeout=5, visibility_timeout=30):
        """Consume messages from a topic.

        Long-polls until messages are available or timeout expires.

        Args:
            topic: Topic name.
            group: Consumer group name.
            max_messages: Max messages to return (1-100).
            timeout: Long-poll timeout in seconds (0-30).
            visibility_timeout: Lease duration in seconds (5-300).

        Returns:
            List of message dicts.
        """
        params = {
            "group": group,
            "max": max_messages,
            "timeout": timeout,
            "visibility_timeout": visibility_timeout,
        }
        # HTTP timeout should be longer than the long-poll timeout
        http_timeout = max(timeout + 5, self.timeout)
        resp = self._session.get(
            self._url(f"/topics/{topic}/messages"),
            params=params,
            timeout=http_timeout,
        )
        result = self._handle_response(resp)
        return result.get("messages", [])

    def ack(self, message_id, group):
        """Acknowledge a consumed message.

        Args:
            message_id: The message ID to acknowledge.
            group: Consumer group name.

        Returns:
            Dict with status and message_id.
        """
        resp = self._session.post(
            self._url(f"/messages/{message_id}/ack"),
            json={"group": group},
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    # --- Request-Reply ---

    def request(self, topic, payload, headers=None, timeout=30):
        """Send a request and wait for a reply.

        Publishes a message with a correlation_id, then blocks until
        a reply is received or timeout expires.

        Args:
            topic: Topic to send the request to.
            payload: Request payload (dict).
            headers: Optional request headers (dict of str->str).
            timeout: Max seconds to wait for reply (1-300).

        Returns:
            Dict with request_id, correlation_id, and reply.

        Raises:
            TimeoutError: If no reply is received within timeout.
        """
        body = {"payload": payload, "timeout": timeout}
        if headers:
            body["headers"] = headers
        # HTTP timeout must be longer than the server-side timeout
        http_timeout = timeout + 5
        resp = self._session.post(
            self._url(f"/topics/{topic}/request"),
            json=body,
            timeout=http_timeout,
        )
        return self._handle_response(resp)

    def reply(self, message_id, payload, headers=None, group=None):
        """Reply to a request message.

        Reads the correlation_id from the original message and sends
        the reply back to the requester.

        Args:
            message_id: The request message ID to reply to.
            payload: Reply payload (dict).
            headers: Optional reply headers (dict of str->str).
            group: If set, acknowledges the original message for this group.

        Returns:
            Dict with status, correlation_id, message_id.
        """
        body = {"payload": payload}
        if headers:
            body["headers"] = headers
        if group:
            body["group"] = group
        resp = self._session.post(
            self._url(f"/messages/{message_id}/reply"),
            json=body,
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    # --- v2 Inbox Model ---

    def list_agents(self):
        """List all registered agents.

        Returns:
            List of agent dicts with id and created_at.
        """
        resp = self._session.get(
            self._url("/agents"),
            timeout=self.timeout,
        )
        result = self._handle_response(resp)
        return result.get("agents", [])

    def register_agent(self, agent_id):
        """Register an agent. Creates its inbox. Idempotent.

        Args:
            agent_id: Unique agent identifier.

        Returns:
            Dict with id and created_at.
        """
        resp = self._session.post(
            self._url("/agents"),
            json={"id": agent_id},
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def delete_agent(self, agent_id):
        """Delete an agent.

        Args:
            agent_id: Agent identifier to delete.

        Returns:
            Dict with status and agent_id.
        """
        resp = self._session.delete(
            self._url(f"/agents/{agent_id}"),
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def send(self, to, task_id, from_agent, msg_type, content=None):
        """Send a message to an agent's inbox.

        Args:
            to: Target agent ID.
            task_id: Task/conversation identifier.
            from_agent: Sender agent ID.
            msg_type: One of: request, question, answer, done, failed.
            content: Optional message content (dict).

        Returns:
            Dict with message_id and created_at.
        """
        body = {"task_id": task_id, "from": from_agent, "type": msg_type}
        if content is not None:
            body["content"] = content
        resp = self._session.post(
            self._url(f"/agents/{to}/inbox"),
            json=body,
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def receive(self, agent_id, status=None, task_id=None, timeout=0):
        """Poll an agent's inbox for messages.

        Args:
            agent_id: Agent whose inbox to read.
            status: Filter by status ('unread' or 'acked'). None for all.
            task_id: Filter by task_id. None for all.
            timeout: Long-poll timeout in seconds (0 for immediate).

        Returns:
            List of message dicts.
        """
        params = {}
        if status:
            params["status"] = status
        if task_id:
            params["task_id"] = task_id
        if timeout > 0:
            params["timeout"] = timeout

        http_timeout = max(timeout + 5, self.timeout)
        resp = self._session.get(
            self._url(f"/agents/{agent_id}/inbox"),
            params=params,
            timeout=http_timeout,
        )
        result = self._handle_response(resp)
        return result.get("messages", [])

    def ack_inbox(self, message_id):
        """Acknowledge an inbox message (mark as read).

        Args:
            message_id: The inbox message ID to acknowledge.

        Returns:
            Dict with status and message_id.
        """
        resp = self._session.post(
            self._url(f"/inbox/messages/{message_id}/ack"),
            timeout=self.timeout,
        )
        return self._handle_response(resp)

    def get_task_messages(self, task_id):
        """Get the full conversation history for a task.

        Args:
            task_id: Task/conversation identifier.

        Returns:
            List of message dicts in chronological order.
        """
        resp = self._session.get(
            self._url(f"/tasks/{task_id}/messages"),
            timeout=self.timeout,
        )
        result = self._handle_response(resp)
        return result.get("messages", [])

    def close(self):
        """Close the underlying HTTP session."""
        self._session.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class Agent:
    """High-level agent interface for stream0 inbox communication.

    Wraps Stream0Client with a fixed agent identity for cleaner usage:

        agent = Agent("my-agent", url="http://localhost:8080")
        agent.register()
        agent.send("other-agent", task_id="t1", msg_type="request", content={...})
        messages = agent.receive()
        agent.ack(messages[0]["id"])
    """

    def __init__(self, agent_id, url="http://localhost:8080", api_key=None, timeout=30):
        self.agent_id = agent_id
        self.client = Stream0Client(url, api_key=api_key, timeout=timeout)

    def register(self):
        """Register this agent with stream0."""
        return self.client.register_agent(self.agent_id)

    def send(self, to, task_id, msg_type, content=None):
        """Send a message to another agent's inbox."""
        return self.client.send(to, task_id, self.agent_id, msg_type, content)

    def receive(self, status="unread", task_id=None, timeout=0):
        """Poll this agent's inbox."""
        return self.client.receive(self.agent_id, status=status, task_id=task_id, timeout=timeout)

    def ack(self, message_id):
        """Acknowledge a message."""
        return self.client.ack_inbox(message_id)

    def history(self, task_id):
        """Get full conversation history for a task."""
        return self.client.get_task_messages(task_id)

    def close(self):
        """Close the underlying client."""
        self.client.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
