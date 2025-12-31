"""
AiMesh SDK Exceptions
"""


class AiMeshError(Exception):
    """Base exception for AiMesh SDK."""
    pass


class ConnectionError(AiMeshError):
    """Connection to AiMesh server failed."""
    pass


class RateLimitError(AiMeshError):
    """Rate limit exceeded."""
    
    def __init__(self, message: str, retry_after: int = 0):
        super().__init__(message)
        self.retry_after = retry_after


class BudgetExceededError(AiMeshError):
    """Agent budget exceeded."""
    
    def __init__(self, agent_id: str, required: float, available: float):
        super().__init__(
            f"Budget exceeded for agent {agent_id}: "
            f"required {required}, available {available}"
        )
        self.agent_id = agent_id
        self.required = required
        self.available = available


class ValidationError(AiMeshError):
    """Message validation failed."""
    
    def __init__(self, field: str, message: str):
        super().__init__(f"Validation error for {field}: {message}")
        self.field = field


class TimeoutError(AiMeshError):
    """Request timed out."""
    pass


class EndpointError(AiMeshError):
    """No healthy endpoints available."""
    pass
