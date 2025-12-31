# AiMesh Python SDK

Python client library for [AiMesh](https://github.com/YASSERRMD/AiMesh) - High-Performance AI Agent Message Queue.

## Installation

```bash
pip install aimesh
```

Or install from source:

```bash
cd sdk/python
pip install -e .
```

## Quick Start

```python
from aimesh import AiMeshClient, Message, EndpointMetrics

# Create client
client = AiMeshClient("http://localhost:9000")

# Register an AI endpoint
client.register_endpoint(EndpointMetrics(
    endpoint_id="gpt-4",
    capacity=1000,
    current_load=0,
    cost_per_1k_tokens=30.0,
    latency_p99_ms=500.0,
    error_rate=0.01,
))

# Set agent budget
client.set_budget("my-agent", tokens=10000)

# Send a message
msg = Message(
    agent_id="my-agent",
    payload=b"Hello, AI!",
    priority=50,
)

ack = client.send_message(msg)
print(f"Processed in {ack.processing_latency_ms}ms")
print(f"Tokens used: {ack.tokens_used}")
```

## Features

- Send and receive AI agent messages
- Register and manage AI endpoints
- Budget management with token tracking
- Batch message processing
- Health checks and metrics

## API Reference

### AiMeshClient

```python
client = AiMeshClient(
    base_url="http://localhost:9000",
    timeout=30,
    api_key=None,  # Optional API key
)
```

#### Message Operations

- `send_message(message)` - Send a single message
- `send_batch(messages)` - Send multiple messages

#### Endpoint Operations

- `register_endpoint(metrics)` - Register an AI endpoint
- `list_endpoints()` - List all endpoints
- `remove_endpoint(endpoint_id)` - Remove an endpoint

#### Budget Operations

- `set_budget(agent_id, tokens)` - Set token budget
- `get_budget(agent_id)` - Get budget info
- `reset_budget(agent_id)` - Reset budget

#### Stats Operations

- `get_stats()` - Get system statistics
- `get_metrics()` - Get Prometheus metrics
- `health_check()` - Check server health

## Error Handling

```python
from aimesh import (
    AiMeshError,
    ConnectionError,
    RateLimitError,
    BudgetExceededError,
    ValidationError,
)

try:
    ack = client.send_message(msg)
except RateLimitError as e:
    print(f"Rate limited, retry after {e.retry_after}s")
except BudgetExceededError as e:
    print(f"Budget exceeded: {e.required} > {e.available}")
except ConnectionError as e:
    print(f"Connection failed: {e}")
```

## License

MIT License
