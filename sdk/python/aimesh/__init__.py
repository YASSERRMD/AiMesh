"""
AiMesh Python SDK

High-performance client for AiMesh AI agent message queue.
"""

from .client import AiMeshClient
from .models import (
    Message,
    RoutingDecision,
    Acknowledgment,
    EndpointMetrics,
    BudgetInfo,
)
from .exceptions import (
    AiMeshError,
    ConnectionError,
    RateLimitError,
    BudgetExceededError,
    ValidationError,
)

__version__ = "0.1.0"
__all__ = [
    "AiMeshClient",
    "Message",
    "RoutingDecision",
    "Acknowledgment",
    "EndpointMetrics",
    "BudgetInfo",
    "AiMeshError",
    "ConnectionError",
    "RateLimitError",
    "BudgetExceededError",
    "ValidationError",
]
