/**
 * AiMesh Node.js SDK Errors
 */

export class AiMeshError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'AiMeshError';
    }
}

export class ConnectionError extends AiMeshError {
    constructor(message: string) {
        super(message);
        this.name = 'ConnectionError';
    }
}

export class RateLimitError extends AiMeshError {
    public retryAfter: number;

    constructor(message: string, retryAfter: number = 0) {
        super(message);
        this.name = 'RateLimitError';
        this.retryAfter = retryAfter;
    }
}

export class BudgetExceededError extends AiMeshError {
    public agentId: string;
    public required: number;
    public available: number;

    constructor(agentId: string, required: number, available: number) {
        super(`Budget exceeded for agent ${agentId}: required ${required}, available ${available}`);
        this.name = 'BudgetExceededError';
        this.agentId = agentId;
        this.required = required;
        this.available = available;
    }
}

export class ValidationError extends AiMeshError {
    public field: string;

    constructor(field: string, message: string) {
        super(`Validation error for ${field}: ${message}`);
        this.name = 'ValidationError';
        this.field = field;
    }
}

export class TimeoutError extends AiMeshError {
    constructor(message: string) {
        super(message);
        this.name = 'TimeoutError';
    }
}
