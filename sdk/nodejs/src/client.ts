/**
 * AiMesh Node.js SDK Client
 */

import * as http from 'http';
import * as https from 'https';
import { URL } from 'url';

import {
    Message,
    Acknowledgment,
    EndpointMetrics,
    BudgetInfo,
    HealthStatus,
    AiMeshClientConfig,
    Stats,
} from './types';

import {
    AiMeshError,
    ConnectionError,
    RateLimitError,
    BudgetExceededError,
    ValidationError,
} from './errors';

function generateUUID(): string {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
        const r = (Math.random() * 16) | 0;
        const v = c === 'x' ? r : (r & 0x3) | 0x8;
        return v.toString(16);
    });
}

export class AiMeshClient {
    private baseUrl: string;
    private timeout: number;
    private apiKey?: string;

    constructor(config: AiMeshClientConfig = {}) {
        this.baseUrl = (config.baseUrl || 'http://localhost:9000').replace(/\/$/, '');
        this.timeout = config.timeout || 30000;
        this.apiKey = config.apiKey;
    }

    private async request<T>(method: string, path: string, data?: object): Promise<T> {
        const url = new URL(path, this.baseUrl);
        const isHttps = url.protocol === 'https:';
        const transport = isHttps ? https : http;

        const options: http.RequestOptions = {
            hostname: url.hostname,
            port: url.port || (isHttps ? 443 : 80),
            path: url.pathname,
            method,
            timeout: this.timeout,
            headers: {
                'Content-Type': 'application/json',
                'Accept': 'application/json',
            },
        };

        if (this.apiKey) {
            options.headers!['Authorization'] = `Bearer ${this.apiKey}`;
        }

        return new Promise((resolve, reject) => {
            const req = transport.request(options, (res) => {
                let body = '';
                res.on('data', (chunk) => (body += chunk));
                res.on('end', () => {
                    if (res.statusCode === 429) {
                        const retryAfter = parseInt(res.headers['retry-after'] as string) || 60;
                        reject(new RateLimitError(`Rate limit exceeded`, retryAfter));
                        return;
                    }

                    if (res.statusCode === 402) {
                        reject(new BudgetExceededError('unknown', 0, 0));
                        return;
                    }

                    if (res.statusCode === 400) {
                        reject(new ValidationError('request', body));
                        return;
                    }

                    if (res.statusCode && res.statusCode >= 400) {
                        reject(new AiMeshError(`HTTP ${res.statusCode}: ${body}`));
                        return;
                    }

                    try {
                        resolve(JSON.parse(body) as T);
                    } catch {
                        resolve(body as unknown as T);
                    }
                });
            });

            req.on('error', (err) => {
                reject(new ConnectionError(`Failed to connect: ${err.message}`));
            });

            req.on('timeout', () => {
                req.destroy();
                reject(new AiMeshError('Request timed out'));
            });

            if (data) {
                req.write(JSON.stringify(data));
            }
            req.end();
        });
    }

    // Message Operations

    async sendMessage(message: Message): Promise<Acknowledgment> {
        const payload = {
            agent_id: message.agentId,
            message_id: message.messageId || generateUUID(),
            payload: Buffer.isBuffer(message.payload)
                ? message.payload.toString('hex')
                : Buffer.from(message.payload).toString('hex'),
            estimated_cost_tokens: message.estimatedCostTokens || 0,
            budget_tokens: message.budgetTokens || 1000,
            deadline_ms: message.deadlineMs || Date.now() + 60000,
            task_graph_id: message.taskGraphId || '',
            dependencies: message.dependencies || [],
            priority: message.priority || 50,
            dedup_context: message.dedupContext || '',
            trace_id: message.traceId || '',
            metadata: message.metadata || {},
            timestamp: message.timestamp || Date.now() * 1000000,
        };

        const response = await this.request<any>('POST', '/messages', payload);

        return {
            originalMessageId: response.original_message_id,
            status: response.status || 'success',
            tokensUsed: response.tokens_used || 0,
            processingLatencyMs: response.processing_latency_ms || 0,
            error: response.error,
            result: response.result ? Buffer.from(response.result, 'hex') : undefined,
        };
    }

    async sendBatch(messages: Message[]): Promise<Acknowledgment[]> {
        const payload = {
            messages: messages.map((m) => ({
                agent_id: m.agentId,
                message_id: m.messageId || generateUUID(),
                payload: Buffer.isBuffer(m.payload)
                    ? m.payload.toString('hex')
                    : Buffer.from(m.payload).toString('hex'),
                priority: m.priority || 50,
                budget_tokens: m.budgetTokens || 1000,
            })),
        };

        const response = await this.request<any>('POST', '/messages/batch', payload);

        return (response.acknowledgments || []).map((ack: any) => ({
            originalMessageId: ack.original_message_id,
            status: ack.status || 'success',
            tokensUsed: ack.tokens_used || 0,
            processingLatencyMs: ack.processing_latency_ms || 0,
        }));
    }

    // Endpoint Operations

    async registerEndpoint(metrics: EndpointMetrics): Promise<boolean> {
        await this.request('POST', '/endpoints', {
            endpoint_id: metrics.endpointId,
            capacity: metrics.capacity,
            current_load: metrics.currentLoad,
            cost_per_1k_tokens: metrics.costPer1kTokens,
            latency_p99_ms: metrics.latencyP99Ms,
            error_rate: metrics.errorRate,
            health_status: metrics.healthStatus || 'healthy',
        });
        return true;
    }

    async listEndpoints(): Promise<EndpointMetrics[]> {
        const response = await this.request<any>('GET', '/endpoints');
        return (response.endpoints || []).map((e: any) => ({
            endpointId: e.endpoint_id,
            capacity: e.capacity,
            currentLoad: e.current_load,
            costPer1kTokens: e.cost_per_1k_tokens,
            latencyP99Ms: e.latency_p99_ms,
            errorRate: e.error_rate,
            healthStatus: e.health_status,
        }));
    }

    async removeEndpoint(endpointId: string): Promise<boolean> {
        await this.request('DELETE', `/endpoints/${endpointId}`);
        return true;
    }

    // Budget Operations

    async setBudget(agentId: string, tokens: number, resetAt?: number): Promise<boolean> {
        await this.request('POST', '/budgets', {
            agent_id: agentId,
            tokens,
            reset_at: resetAt,
        });
        return true;
    }

    async getBudget(agentId: string): Promise<BudgetInfo> {
        const response = await this.request<any>('GET', `/budgets/${agentId}`);
        return {
            agentId: response.agent_id,
            initialTokens: response.initial_tokens,
            remainingTokens: response.remaining_tokens,
            consumptionRate: response.consumption_rate || 0,
            resetAt: response.reset_at,
        };
    }

    async resetBudget(agentId: string): Promise<boolean> {
        await this.request('POST', `/budgets/${agentId}/reset`);
        return true;
    }

    // Stats Operations

    async getStats(): Promise<Stats> {
        return this.request<Stats>('GET', '/stats');
    }

    async getMetrics(): Promise<string> {
        return this.request<string>('GET', '/metrics');
    }

    async healthCheck(): Promise<HealthStatus> {
        return this.request<HealthStatus>('GET', '/health');
    }
}
