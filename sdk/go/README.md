# AiMesh Go SDK

Go client library for [AiMesh](https://github.com/YASSERRMD/AiMesh) - High-Performance AI Agent Message Queue.

## Installation

```bash
go get github.com/YASSERRMD/AiMesh/sdk/go
```

## Quick Start

```go
package main

import (
    "fmt"
    "log"

    "github.com/YASSERRMD/AiMesh/sdk/go/aimesh"
)

func main() {
    // Create client
    client := aimesh.NewClient(aimesh.ClientConfig{
        BaseURL: "http://localhost:9000",
    })

    // Check health
    health, err := client.HealthCheck()
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Server status: %s\n", health.Status)

    // Register an AI endpoint
    err = client.RegisterEndpoint(&aimesh.EndpointMetrics{
        EndpointID:      "gpt-4",
        Capacity:        1000,
        CurrentLoad:     0,
        CostPer1kTokens: 30.0,
        LatencyP99Ms:    500.0,
        ErrorRate:       0.01,
        HealthStatus:    "healthy",
    })
    if err != nil {
        log.Fatal(err)
    }

    // Set agent budget
    err = client.SetBudget("my-agent", 10000)
    if err != nil {
        log.Fatal(err)
    }

    // Send a message
    msg := aimesh.NewMessage("my-agent", []byte("Hello, AI!"))
    msg.Priority = 50

    ack, err := client.SendMessage(msg)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Processed in %dms\n", ack.ProcessingLatencyMs)
    fmt.Printf("Tokens used: %.2f\n", ack.TokensUsed)
}
```

## Features

- Send and receive AI agent messages
- Register and manage AI endpoints
- Budget management with token tracking
- Health checks and metrics
- Error handling with typed errors

## API Reference

### Client

```go
client := aimesh.NewClient(aimesh.ClientConfig{
    BaseURL: "http://localhost:9000",
    Timeout: 30 * time.Second,
    APIKey:  "",  // Optional API key
})
```

#### Message Operations

- `SendMessage(msg)` - Send a single message

#### Endpoint Operations

- `RegisterEndpoint(metrics)` - Register an AI endpoint
- `ListEndpoints()` - List all endpoints
- `RemoveEndpoint(endpointID)` - Remove an endpoint

#### Budget Operations

- `SetBudget(agentID, tokens)` - Set token budget
- `GetBudget(agentID)` - Get budget info
- `ResetBudget(agentID)` - Reset budget

#### Stats Operations

- `HealthCheck()` - Check server health
- `GetMetrics()` - Get Prometheus metrics

## Error Handling

```go
import "errors"

ack, err := client.SendMessage(msg)
if err != nil {
    if errors.Is(err, aimesh.ErrRateLimit) {
        fmt.Println("Rate limited, try again later")
    } else if errors.Is(err, aimesh.ErrBudgetExceeded) {
        fmt.Println("Budget exceeded")
    } else if errors.Is(err, aimesh.ErrConnection) {
        fmt.Println("Connection failed")
    }
}
```

## License

MIT License
