# AiMesh

**High-Performance AI Agent Message Queue with Cost-Aware Routing**

AiMesh is a Rust-based message queue designed for AI agent orchestration, featuring cost-aware routing, semantic deduplication, and scatter-gather workflows.

## Features

- 🚀 **High Performance**: Targets 5M+ msgs/sec with <1ms P99 latency
- 💰 **Cost-Aware Routing**: Automatically routes to the most cost-effective endpoint
- 🔄 **Semantic Deduplication**: Blake3-based caching to avoid duplicate API calls
- 📊 **Budget Enforcement**: Per-agent token budgets with real-time tracking
- 🌐 **QUIC Transport**: Low-latency UDP-based transport (coming soon)
- 📈 **Observability**: Built-in metrics with P50/P99/P99.9 latencies

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        AiMesh                                │
├─────────────┬─────────────┬───────────────┬────────────────┤
│  Protocol   │   Router    │   Storage     │  Observability │
│  (Protobuf) │ (Cost-Aware)│ (Barq-DB)     │   (Metrics)    │
├─────────────┴─────────────┴───────────────┴────────────────┤
│                      Barq Ecosystem                         │
│  ┌─────────────────────┐  ┌──────────────────────────────┐ │
│  │      Barq-DB        │  │       Barq-GraphDB           │ │
│  │  (Vector Database)  │  │    (Graph Relationships)     │ │
│  │  - Message storage  │  │    - Agent relationships     │ │
│  │  - Semantic search  │  │    - Task dependencies       │ │
│  │  - Dedup cache      │  │    - Hybrid queries          │ │
│  └─────────────────────┘  └──────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
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
# Clone the repository
git clone https://github.com/YASSERRMD/AiMesh.git
cd AiMesh

# Build
cargo build --release

# Run
cargo run --release
```

### Example Usage

```rust
use aimesh::{AiMesh, AiMeshConfig, AiMessage};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create AiMesh instance
    let mesh = AiMesh::new(AiMeshConfig::default())?;
    
    // Register an AI model endpoint
    mesh.router.register_endpoint(aimesh::EndpointMetrics {
        endpoint_id: "gpt-4".into(),
        capacity: 1000,
        current_load: 0,
        cost_per_1k_tokens: 30.0,
        latency_p99_ms: 500.0,
        error_rate: 0.01,
        ..Default::default()
    });
    
    // Set agent budget (10,000 tokens)
    mesh.router.set_budget("my-agent", 10000.0, i64::MAX);
    
    // Process a message
    let msg = AiMessage::new(
        "my-agent".into(),
        b"Hello, AI!".to_vec(),
        1000.0, // budget
        i64::MAX, // deadline
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

The cost-aware router scores endpoints using:

```
score = (cost × 0.4) + (load × 0.3) + (latency × 0.3)
```

Lower score = better endpoint. The router:
1. Checks agent budget
2. Filters healthy endpoints
3. Scores all endpoints
4. Selects the lowest score
5. Provides fallback endpoints

## Development Phases

- [x] **Phase 1A**: Architecture & Design
- [ ] **Phase 1B**: Core Implementation (QUIC transport)
- [ ] **Phase 1C**: Integration & Testing
- [ ] **Phase 1D**: Beta Launch
- [ ] **Phase 2**: Enterprise Features
- [ ] **Phase 3**: SDKs & Integrations
- [ ] **Phase 4**: Global Scale

## License

MIT License - see [LICENSE](LICENSE) for details.

## Author

**YASSERRMD** - [GitHub](https://github.com/YASSERRMD)
