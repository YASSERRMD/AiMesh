# AiMesh Protocol Documentation

## Message Types

### AIMessage
The core message type for AI agent communication.

| Field | Type | Description |
|-------|------|-------------|
| `agent_id` | string | Unique agent identifier (pattern: `[a-z0-9_-]+`) |
| `message_id` | string | UUID v7 for time-ordered uniqueness |
| `payload` | bytes | Binary payload (max 1MB) |
| `estimated_cost_tokens` | double | Estimated token cost |
| `budget_tokens` | double | Maximum allowed tokens |
| `deadline_ms` | int64 | Request deadline (epoch ms) |
| `task_graph_id` | string | Scatter-gather task ID |
| `dependencies` | string[] | Message IDs that must complete first |
| `priority` | int32 | 0-100 priority (higher = more urgent) |
| `dedup_context` | string | Semantic deduplication context |
| `trace_id` | string | Distributed tracing ID |
| `metadata` | map | Arbitrary key-value pairs |
| `timestamp` | int64 | Creation time (nanoseconds) |

### Validation Rules

1. **agent_id**: Must match `^[a-z0-9_-]+$`, non-empty
2. **payload**: Max 1MB (1,048,576 bytes)
3. **budget_tokens**: Must be > 0
4. **deadline_ms**: Must be in the future (if set)
5. **priority**: Must be 0-100

### RoutingDecision
Represents a routing decision from the CostAwareRouter.

| Field | Type | Description |
|-------|------|-------------|
| `message_id` | string | Message this decision is for |
| `target_endpoint` | string | Selected endpoint URI |
| `estimated_latency_ms` | int32 | Expected latency |
| `estimated_cost` | double | Expected cost |
| `routing_reason` | string | Human-readable reason |
| `fallback_endpoints` | string[] | Backup endpoints |
| `score_breakdown` | RoutingScore | Score components |

### EndpointMetrics
Metrics for routing decisions.

| Field | Type | Description |
|-------|------|-------------|
| `endpoint_id` | string | Unique endpoint ID |
| `capacity` | uint32 | Max concurrent requests |
| `current_load` | uint32 | Current requests |
| `cost_per_1k_tokens` | double | Cost in cents |
| `latency_p99_ms` | float | P99 latency |
| `error_rate` | float | Error rate (0.0-1.0) |
| `health_status` | HealthStatus | HEALTHY, DEGRADED, UNHEALTHY |

## Routing Algorithm

The router scores endpoints using weighted factors:

```
score = (cost × 0.4) + (load × 0.3) + (latency × 0.3)
```

- **cost**: `cost_per_1k_tokens × 0.4`
- **load**: `(current_load / capacity) × 100 × 0.3`
- **latency**: `latency_p99_ms × 0.3`

Lower score = better endpoint.

## Deduplication

Semantic deduplication uses Blake3 hashing:

```
hash = blake3(payload + dedup_context)
```

Cached responses are returned for matching hashes within TTL.

## Wire Format

Messages use Protocol Buffers serialization:
- Target: <100ns serialization
- Target: <100ns deserialization
- Validation: <1μs

## Transport

QUIC-based transport with:
- TLS 1.3 encryption
- Length-prefixed framing (4-byte big-endian length + data)
- Connection pooling and reuse
- 10MB flow control windows
