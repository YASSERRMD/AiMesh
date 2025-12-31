/**
 * AiMesh Node.js SDK Types
 */

export interface Message {
    agentId: string;
    payload: Buffer | string;
    messageId?: string;
    estimatedCostTokens?: number;
    budgetTokens?: number;
    deadlineMs?: number;
    taskGraphId?: string;
    dependencies?: string[];
    priority?: number;
    dedupContext?: string;
    traceId?: string;
    metadata?: Record<string, string>;
    timestamp?: number;
}

export interface RoutingScore {
    costScore: number;
    loadScore: number;
    latencyScore: number;
    totalScore: number;
}

export interface RoutingDecision {
    messageId: string;
    targetEndpoint: string;
    estimatedLatencyMs: number;
    estimatedCost: number;
    routingReason: string;
    fallbackEndpoints: string[];
    scoreBreakdown?: RoutingScore;
}

export interface Acknowledgment {
    originalMessageId: string;
    status: 'success' | 'failed';
    tokensUsed: number;
    processingLatencyMs: number;
    error?: string;
    result?: Buffer;
}

export interface EndpointMetrics {
    endpointId: string;
    capacity: number;
    currentLoad: number;
    costPer1kTokens: number;
    latencyP99Ms: number;
    errorRate: number;
    healthStatus?: 'healthy' | 'degraded' | 'unhealthy';
}

export interface BudgetInfo {
    agentId: string;
    initialTokens: number;
    remainingTokens: number;
    consumptionRate: number;
    resetAt?: number;
}

export interface HealthStatus {
    status: string;
    uptimeSecs: number;
    messagesTotal: number;
    endpointsHealthy: number;
    endpointsTotal: number;
}

export interface AiMeshClientConfig {
    baseUrl?: string;
    timeout?: number;
    apiKey?: string;
}

export interface Stats {
    messagesTotal: number;
    messagesSuccess: number;
    messagesFailed: number;
    tokensConsumed: number;
    uptimeSecs: number;
    throughputPerSec: number;
}
