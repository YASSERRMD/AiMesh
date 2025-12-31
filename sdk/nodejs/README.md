# AiMesh Node.js SDK

Node.js/TypeScript client library for [AiMesh](https://github.com/YASSERRMD/AiMesh) - High-Performance AI Agent Message Queue.

## Installation

```bash
npm install @aimesh/sdk
```

## Quick Start

```typescript
import { AiMeshClient, Message, EndpointMetrics } from '@aimesh/sdk';

// Create client
const client = new AiMeshClient({ baseUrl: 'http://localhost:9000' });

// Register an AI endpoint
await client.registerEndpoint({
  endpointId: 'gpt-4',
  capacity: 1000,
  currentLoad: 0,
  costPer1kTokens: 30.0,
  latencyP99Ms: 500.0,
  errorRate: 0.01,
});

// Set agent budget
await client.setBudget('my-agent', 10000);

// Send a message
const ack = await client.sendMessage({
  agentId: 'my-agent',
  payload: Buffer.from('Hello, AI!'),
  priority: 50,
});

console.log(`Processed in ${ack.processingLatencyMs}ms`);
console.log(`Tokens used: ${ack.tokensUsed}`);
```

## Features

- Send and receive AI agent messages
- Register and manage AI endpoints
- Budget management with token tracking
- Batch message processing
- TypeScript support with full type definitions
- Health checks and metrics

## API Reference

### AiMeshClient

```typescript
const client = new AiMeshClient({
  baseUrl: 'http://localhost:9000',
  timeout: 30000,
  apiKey: undefined,  // Optional API key
});
```

#### Message Operations

- `sendMessage(message)` - Send a single message
- `sendBatch(messages)` - Send multiple messages

#### Endpoint Operations

- `registerEndpoint(metrics)` - Register an AI endpoint
- `listEndpoints()` - List all endpoints
- `removeEndpoint(endpointId)` - Remove an endpoint

#### Budget Operations

- `setBudget(agentId, tokens)` - Set token budget
- `getBudget(agentId)` - Get budget info
- `resetBudget(agentId)` - Reset budget

#### Stats Operations

- `getStats()` - Get system statistics
- `getMetrics()` - Get Prometheus metrics
- `healthCheck()` - Check server health

## Error Handling

```typescript
import { 
  AiMeshError, 
  ConnectionError, 
  RateLimitError, 
  BudgetExceededError 
} from '@aimesh/sdk';

try {
  const ack = await client.sendMessage(msg);
} catch (error) {
  if (error instanceof RateLimitError) {
    console.log(`Rate limited, retry after ${error.retryAfter}s`);
  } else if (error instanceof BudgetExceededError) {
    console.log(`Budget exceeded: ${error.required} > ${error.available}`);
  } else if (error instanceof ConnectionError) {
    console.log(`Connection failed: ${error.message}`);
  }
}
```

## License

MIT License
