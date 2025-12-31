"""
AiMesh Python SDK Example

Demonstrates basic usage of the AiMesh client.
"""

from aimesh import AiMeshClient, Message, EndpointMetrics


def main():
    # Create client
    client = AiMeshClient("http://localhost:9000")
    
    # Check health
    try:
        health = client.health_check()
        print(f"Server status: {health['status']}")
    except Exception as e:
        print(f"Server not available: {e}")
        return
    
    # Register endpoints
    endpoints = [
        EndpointMetrics(
            endpoint_id="gpt-4",
            capacity=1000,
            current_load=0,
            cost_per_1k_tokens=30.0,
            latency_p99_ms=500.0,
            error_rate=0.01,
        ),
        EndpointMetrics(
            endpoint_id="claude-3",
            capacity=500,
            current_load=0,
            cost_per_1k_tokens=15.0,
            latency_p99_ms=300.0,
            error_rate=0.005,
        ),
    ]
    
    for ep in endpoints:
        client.register_endpoint(ep)
        print(f"Registered endpoint: {ep.endpoint_id}")
    
    # Set budget
    client.set_budget("example-agent", tokens=50000)
    
    # Send messages
    messages = [
        Message(
            agent_id="example-agent",
            payload=b"What is the capital of France?",
            priority=50,
        ),
        Message(
            agent_id="example-agent",
            payload=b"Explain quantum computing in simple terms.",
            priority=75,  # Higher priority
        ),
    ]
    
    for msg in messages:
        ack = client.send_message(msg)
        print(f"Message {msg.message_id[:8]}...")
        print(f"  Status: {ack.status}")
        print(f"  Latency: {ack.processing_latency_ms}ms")
        print(f"  Tokens: {ack.tokens_used}")
    
    # Check budget
    budget = client.get_budget("example-agent")
    print(f"\nBudget remaining: {budget.remaining_tokens}")
    print(f"Utilization: {budget.utilization_percent:.1f}%")
    
    # Get stats
    stats = client.get_stats()
    print(f"\nSystem stats:")
    print(f"  Total messages: {stats.get('messages_total', 0)}")
    print(f"  Uptime: {stats.get('uptime_secs', 0)}s")


if __name__ == "__main__":
    main()
