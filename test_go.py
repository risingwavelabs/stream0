#!/usr/bin/env python3
"""Test AgentBus Go implementation."""
import asyncio
import aiohttp
import json
import sys
import uuid

BASE_URL = "http://127.0.0.1:8080"


def unique_topic(prefix="test"):
    """Generate unique topic name."""
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


async def test_health():
    """Test health endpoint."""
    print("Test: Health check")
    async with aiohttp.ClientSession() as session:
        async with session.get(f"{BASE_URL}/health") as resp:
            data = await resp.json()
            assert data["status"] == "healthy"
            assert data["version"] == "0.1.0-go"
            print("  ✓ Health endpoint works")
            return True


async def test_topic_crud():
    """Test topic create, list, get."""
    print("\nTest: Topic CRUD")
    async with aiohttp.ClientSession() as session:
        topic = unique_topic("crud")

        # Create
        async with session.post(f"{BASE_URL}/topics", json={
            "name": topic,
            "retention_days": 7
        }) as resp:
            assert resp.status == 201
            data = await resp.json()
            print(f"  ✓ Created topic: {data['name']}")

        # List
        async with session.get(f"{BASE_URL}/topics") as resp:
            data = await resp.json()
            assert len(data) >= 1
            print(f"  ✓ Listed {len(data)} topics")

        # Get
        async with session.get(f"{BASE_URL}/topics/{topic}") as resp:
            data = await resp.json()
            assert data["name"] == topic
            print(f"  ✓ Got topic info")

        return True


async def test_produce_consume():
    """Test produce and consume."""
    print("\nTest: Produce and Consume")
    async with aiohttp.ClientSession() as session:
        topic = unique_topic("pc")

        # Create topic
        await session.post(f"{BASE_URL}/topics", json={"name": topic})

        # Produce
        async with session.post(f"{BASE_URL}/topics/{topic}/messages", json={
            "payload": {"test": "data"},
            "headers": {"x-id": "123"}
        }) as resp:
            assert resp.status == 201
            result = await resp.json()
            msg_id = result["message_id"]
            print(f"  ✓ Produced message: {msg_id[:20]}...")

        # Consume
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "test-group",
            "max": 10,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            assert "messages" in data
            assert len(data["messages"]) == 1
            msg = data["messages"][0]
            assert msg["payload"]["test"] == "data"
            print(f"  ✓ Consumed message with correct payload")

        # Ack
        async with session.post(f"{BASE_URL}/messages/{msg_id}/ack", json={
            "group": "test-group"
        }) as resp:
            assert resp.status == 200
            print(f"  ✓ Acknowledged message")

        # Verify empty
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "test-group",
            "max": 10,
            "timeout": 1
        }) as resp:
            data = await resp.json()
            assert len(data["messages"]) == 0
            print(f"  ✓ No messages after ack")

        return True


async def test_consumer_groups():
    """Test consumer group load balancing."""
    print("\nTest: Consumer Groups (Load Balancing)")
    async with aiohttp.ClientSession() as session:
        topic = unique_topic("groups")

        # Create topic
        await session.post(f"{BASE_URL}/topics", json={"name": topic})

        # Produce 5 messages
        for i in range(5):
            await session.post(f"{BASE_URL}/topics/{topic}/messages", json={
                "payload": {"seq": i}
            })
        print(f"  ✓ Produced 5 messages")

        # Consumer 1 (same group)
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "workers",
            "max": 10,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            msgs1 = data["messages"]
            print(f"  Consumer 1 got: {len(msgs1)} messages")

        # Consumer 2 (same group - should get rest)
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "workers",
            "max": 10,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            msgs2 = data["messages"]
            print(f"  Consumer 2 got: {len(msgs2)} messages")

        # Verify no duplicates
        ids1 = {m["id"] for m in msgs1}
        ids2 = {m["id"] for m in msgs2}
        assert len(ids1 & ids2) == 0, "Duplicate messages found!"
        assert len(msgs1) + len(msgs2) == 5, f"Expected 5 total, got {len(msgs1) + len(msgs2)}"

        print(f"  ✓ All 5 messages claimed uniquely")
        return True


async def test_different_groups():
    """Test different groups see same messages (broadcast)."""
    print("\nTest: Different Groups (Broadcast)")
    async with aiohttp.ClientSession() as session:
        topic = unique_topic("broadcast")

        # Create topic
        await session.post(f"{BASE_URL}/topics", json={"name": topic})

        # Produce 3 messages
        for i in range(3):
            await session.post(f"{BASE_URL}/topics/{topic}/messages", json={
                "payload": {"seq": i}
            })
        print(f"  ✓ Produced 3 messages")

        # Group A
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "group-a",
            "max": 10,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            msgs_a = data["messages"]
            print(f"  Group A got: {len(msgs_a)} messages")

        # Group B
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "group-b",
            "max": 10,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            msgs_b = data["messages"]
            print(f"  Group B got: {len(msgs_b)} messages")

        assert len(msgs_a) == 3 and len(msgs_b) == 3
        print(f"  ✓ Both groups got all 3 messages")
        return True


async def test_visibility_timeout():
    """Test visibility timeout and redelivery."""
    print("\nTest: Visibility Timeout (Message Recovery)")
    async with aiohttp.ClientSession() as session:
        topic = unique_topic("timeout")

        # Create topic
        await session.post(f"{BASE_URL}/topics", json={"name": topic})

        # Produce
        await session.post(f"{BASE_URL}/topics/{topic}/messages", json={
            "payload": {"test": "timeout"}
        })
        print(f"  ✓ Produced 1 message")

        # Consumer 1 claims (5s visibility)
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "timeout-group",
            "max": 1,
            "timeout": 1,
            "visibility_timeout": 5
        }) as resp:
            data = await resp.json()
            msg = data["messages"][0]
            msg_id = msg["id"]
            print(f"  ✓ Consumer 1 claimed (NOT acking)")

        # Wait
        print(f"  Waiting 6 seconds...")
        await asyncio.sleep(6)

        # Consumer 2 should see it
        async with session.get(f"{BASE_URL}/topics/{topic}/messages", params={
            "group": "timeout-group",
            "max": 1,
            "timeout": 2
        }) as resp:
            data = await resp.json()
            assert len(data["messages"]) == 1
            assert data["messages"][0]["id"] == msg_id
            print(f"  ✓ Message redelivered after timeout")
            return True


async def run_all():
    print("=" * 60)
    print("AGENTBUS GO TEST SUITE")
    print("=" * 60)

    tests = [
        ("Health check", test_health),
        ("Topic CRUD", test_topic_crud),
        ("Produce and consume", test_produce_consume),
        ("Consumer groups", test_consumer_groups),
        ("Different groups", test_different_groups),
        ("Visibility timeout", test_visibility_timeout),
    ]

    results = []
    for name, test in tests:
        try:
            result = await test()
            results.append((name, result))
        except Exception as e:
            print(f"  ✗ FAIL: {e}")
            results.append((name, False))

    print("\n" + "=" * 60)
    print("SUMMARY")
    passed = sum(1 for _, r in results if r)
    for name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        print(f"  {status}: {name}")
    print(f"\n{passed}/{len(results)} tests passed")
    print("=" * 60)
    return passed == len(results)


if __name__ == "__main__":
    success = asyncio.run(run_all())
    sys.exit(0 if success else 1)
