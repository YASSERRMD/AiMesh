# AiMesh

A high-performance AI agent message queue with cost-aware routing, built in Rust.

## Overview

AiMesh is a production-ready message queue designed specifically for AI agent orchestration. It provides intelligent routing based on cost, latency, and load, with built-in budget enforcement and semantic deduplication.

## Key Features

- **High Performance**: Targets 5M+ messages/second with sub-millisecond P99 latency
- **Cost-Aware Routing**: Automatically routes messages to the most cost-effective endpoint
- **Budget Enforcement**: Per-agent token budget tracking with real-time enforcement
- **Semantic Deduplication**: Blake3-based caching to prevent duplicate API calls
- **Scatter-Gather Orchestration**: Dependency-aware task execution for complex workflows
- **QUIC Transport**: Modern UDP-based transport with TLS 1.3 encryption
- **Observability**: Built-in metrics with P50/P99/P99.9 latency tracking

## Architecture

```
+------------------------------------------------------------------+
|                           AiMesh                                  |
+----------------+----------------+----------------+----------------+
|    Protocol    |     Router     |    Storage     |  Observability |
|   (Protobuf)   |  (Cost-Aware)  |   (Barq-DB)    |    (Metrics)   |
+----------------+----------------+----------------+----------------+
|                         Barq Ecosystem                            |
| +---------------------------+ +--------------------------------+ |
| |         Barq-DB           | |         Barq-GraphDB           | |
| |    (Vector Database)      | |     (Graph Relationships)      | |
| |    - Message storage      | |     - Agent relationships      | |
| |    - Semantic search      | |     - Task dependencies        | |
| |    - Dedup cache          | |     - Hybrid queries           | |
| +---------------------------+ +--------------------------------+ |
+------------------------------------------------------------------+
```

## Storage Backends

AiMesh uses the Barq ecosystem for storage:

- **[Barq-DB](https://github.com/YASSERRMD/barq-db)**: Rust-based vector database with BM25 + vector hybrid search
- **[Barq-GraphDB](https://github.com/YASSERRMD/barq-graphdb)**: Production-ready hybrid Graph+Vector DB for agentic AI

## Quick Start

### Prerequisites

1. Start Barq-DB (default port 8080):
```bash
docker run -p 8080:8080 yasserrmd/barq-db
```

2. Start Barq-GraphDB (default port 8081):
```bash
docker run -p 8081:8081 -p 50052:50052 yasserrmd/barq-graphdb
```

### Build and Run

```bash
git clone https://github.com/YASSERRMD/AiMesh.git
cd AiMesh
cargo build --release
cargo run --release
```

### Example Usage

```rust
use aimesh::{AiMesh, AiMeshConfig, AiMessage, EndpointMetrics, HealthStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mesh = AiMesh::new(AiMeshConfig::default())?;
    
    // Register an AI model endpoint
    mesh.router.register_endpoint(EndpointMetrics {
        endpoint_id: "gpt-4".into(),
        capacity: 1000,
        current_load: 0,
        cost_per_1k_tokens: 30.0,
        latency_p99_ms: 500.0,
        error_rate: 0.01,
        last_health_check: 0,
        health_status: HealthStatus::Healthy as i32,
    });
    
    // Set agent budget (10,000 tokens)
    mesh.router.set_budget("my-agent", 10000.0, i64::MAX);
    
    // Process a message
    let msg = AiMessage::new(
        "my-agent".into(),
        b"Hello, AI!".to_vec(),
        1000.0,
        i64::MAX,
    );
    
    let ack = mesh.process_message(msg).await?;
    println!("Processed in {}ms", ack.processing_latency_ms);
    
    Ok(())
}
```

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `AIMESH_BIND_ADDR` | `0.0.0.0:9000` | AiMesh server address |
| `BARQ_DB_URL` | `http://localhost:8080` | Barq-DB endpoint |
| `BARQ_GRAPHDB_URL` | `http://localhost:8081` | Barq-GraphDB endpoint |
| `DEDUP_TTL_SECS` | `3600` | Deduplication cache TTL |

## Routing Algorithm

The cost-aware router scores endpoints using weighted factors:

```
score = (cost × 0.4) + (load × 0.3) + (latency × 0.3)
```

Lower score indicates a better endpoint. The router:
1. Validates agent budget
2. Filters healthy endpoints
3. Scores all available endpoints
4. Selects the lowest-scoring endpoint
5. Provides fallback endpoints for resilience

## Testing

Run the test suite:

```bash
cargo test
```

Run with verbose output:

```bash
cargo test -- --nocapture
```

## Benchmarks

```bash
cargo bench
```

## Contributing

We welcome contributions from the community. Please read our [Contributing Guide](CONTRIBUTING.md) before submitting a pull request.

### Ways to Contribute

- Report bugs and request features via GitHub Issues
- Submit pull requests for bug fixes and new features
- Improve documentation
- Share feedback and ideas

## Roadmap

- [x] Phase 1A: Architecture and Design
- [x] Phase 1B: Core Implementation
- [x] Phase 1C: Integration and Testing
- [x] Phase 1D: Final Polish
- [ ] Phase 2: Enterprise Features (Rate Limiting, Multi-tenancy)
- [ ] Phase 3: SDKs (Python, Node.js, Go)
- [ ] Phase 4: Global Scale (Federation, Geo-routing)

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Author

**YASSERRMD** - [GitHub](https://github.com/YASSERRMD)

---

For questions and support, please open an issue on GitHub.
