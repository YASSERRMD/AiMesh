"""
AiMesh SDK Data Models
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional
from datetime import datetime
import uuid
import time


@dataclass
class Message:
    """AI message for agent communication."""
    
    agent_id: str
    payload: bytes
    budget_tokens: float = 1000.0
    deadline_ms: Optional[int] = None
    message_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    priority: int = 50
    dedup_context: str = ""
    trace_id: str = ""
    metadata: Dict[str, str] = field(default_factory=dict)
    estimated_cost_tokens: float = 0.0
    task_graph_id: str = ""
    dependencies: List[str] = field(default_factory=list)
    timestamp: int = field(default_factory=lambda: int(time.time_ns()))
    
    def to_dict(self) -> dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "agent_id": self.agent_id,
            "message_id": self.message_id,
            "payload": self.payload.hex() if isinstance(self.payload, bytes) else self.payload,
            "estimated_cost_tokens": self.estimated_cost_tokens,
            "budget_tokens": self.budget_tokens,
            "deadline_ms": self.deadline_ms or int(time.time() * 1000) + 60000,
            "task_graph_id": self.task_graph_id,
            "dependencies": self.dependencies,
            "priority": self.priority,
            "dedup_context": self.dedup_context,
            "trace_id": self.trace_id,
            "metadata": self.metadata,
            "timestamp": self.timestamp,
        }
    
    @classmethod
    def from_dict(cls, data: dict) -> "Message":
        """Create from dictionary."""
        payload = data.get("payload", b"")
        if isinstance(payload, str):
            payload = bytes.fromhex(payload)
        
        return cls(
            agent_id=data["agent_id"],
            message_id=data.get("message_id", str(uuid.uuid4())),
            payload=payload,
            estimated_cost_tokens=data.get("estimated_cost_tokens", 0.0),
            budget_tokens=data.get("budget_tokens", 1000.0),
            deadline_ms=data.get("deadline_ms"),
            task_graph_id=data.get("task_graph_id", ""),
            dependencies=data.get("dependencies", []),
            priority=data.get("priority", 50),
            dedup_context=data.get("dedup_context", ""),
            trace_id=data.get("trace_id", ""),
            metadata=data.get("metadata", {}),
            timestamp=data.get("timestamp", int(time.time_ns())),
        )


@dataclass
class RoutingScore:
    """Routing score breakdown."""
    cost_score: float
    load_score: float
    latency_score: float
    total_score: float


@dataclass
class RoutingDecision:
    """Routing decision from the router."""
    
    message_id: str
    target_endpoint: str
    estimated_latency_ms: int
    estimated_cost: float
    routing_reason: str
    fallback_endpoints: List[str] = field(default_factory=list)
    score_breakdown: Optional[RoutingScore] = None
    
    @classmethod
    def from_dict(cls, data: dict) -> "RoutingDecision":
        score = None
        if "score_breakdown" in data and data["score_breakdown"]:
            sb = data["score_breakdown"]
            score = RoutingScore(
                cost_score=sb.get("cost_score", 0),
                load_score=sb.get("load_score", 0),
                latency_score=sb.get("latency_score", 0),
                total_score=sb.get("total_score", 0),
            )
        
        return cls(
            message_id=data["message_id"],
            target_endpoint=data["target_endpoint"],
            estimated_latency_ms=data.get("estimated_latency_ms", 0),
            estimated_cost=data.get("estimated_cost", 0.0),
            routing_reason=data.get("routing_reason", ""),
            fallback_endpoints=data.get("fallback_endpoints", []),
            score_breakdown=score,
        )


@dataclass
class Acknowledgment:
    """Acknowledgment for processed messages."""
    
    original_message_id: str
    status: str  # "success" or "failed"
    tokens_used: float
    processing_latency_ms: int
    error: str = ""
    result: bytes = field(default_factory=bytes)
    
    @property
    def is_success(self) -> bool:
        return self.status == "success"
    
    @classmethod
    def from_dict(cls, data: dict) -> "Acknowledgment":
        result = data.get("result", b"")
        if isinstance(result, str):
            result = bytes.fromhex(result) if result else b""
        
        return cls(
            original_message_id=data["original_message_id"],
            status=data.get("status", "success"),
            tokens_used=data.get("tokens_used", 0.0),
            processing_latency_ms=data.get("processing_latency_ms", 0),
            error=data.get("error", ""),
            result=result,
        )


@dataclass
class EndpointMetrics:
    """Metrics for an AI endpoint."""
    
    endpoint_id: str
    capacity: int
    current_load: int
    cost_per_1k_tokens: float
    latency_p99_ms: float
    error_rate: float
    health_status: str = "healthy"
    
    def to_dict(self) -> dict:
        return {
            "endpoint_id": self.endpoint_id,
            "capacity": self.capacity,
            "current_load": self.current_load,
            "cost_per_1k_tokens": self.cost_per_1k_tokens,
            "latency_p99_ms": self.latency_p99_ms,
            "error_rate": self.error_rate,
            "health_status": self.health_status,
        }


@dataclass
class BudgetInfo:
    """Budget information for an agent."""
    
    agent_id: str
    initial_tokens: float
    remaining_tokens: float
    consumption_rate: float = 0.0
    reset_at: Optional[int] = None
    
    @property
    def utilization_percent(self) -> float:
        if self.initial_tokens == 0:
            return 0.0
        return ((self.initial_tokens - self.remaining_tokens) / self.initial_tokens) * 100
    
    @classmethod
    def from_dict(cls, data: dict) -> "BudgetInfo":
        return cls(
            agent_id=data["agent_id"],
            initial_tokens=data.get("initial_tokens", 0.0),
            remaining_tokens=data.get("remaining_tokens", 0.0),
            consumption_rate=data.get("consumption_rate", 0.0),
            reset_at=data.get("reset_at"),
        )
