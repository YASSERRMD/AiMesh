"""
AiMesh Python Client

HTTP client for interacting with AiMesh server.
"""

import json
import time
from typing import Dict, List, Optional, Any
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError

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
    TimeoutError,
)


class AiMeshClient:
    """
    AiMesh Python SDK Client.
    
    Example:
        client = AiMeshClient("http://localhost:9000")
        
        # Send a message
        msg = Message(agent_id="my-agent", payload=b"Hello, AI!")
        ack = client.send_message(msg)
        print(f"Processed in {ack.processing_latency_ms}ms")
    """
    
    def __init__(
        self,
        base_url: str = "http://localhost:9000",
        timeout: int = 30,
        api_key: Optional[str] = None,
    ):
        """
        Initialize AiMesh client.
        
        Args:
            base_url: AiMesh server URL
            timeout: Request timeout in seconds
            api_key: Optional API key for authentication
        """
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.api_key = api_key
    
    def _request(
        self,
        method: str,
        path: str,
        data: Optional[dict] = None,
    ) -> dict:
        """Make HTTP request to AiMesh server."""
        url = f"{self.base_url}{path}"
        
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json",
        }
        
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        
        body = json.dumps(data).encode() if data else None
        
        req = Request(url, data=body, headers=headers, method=method)
        
        try:
            with urlopen(req, timeout=self.timeout) as response:
                return json.loads(response.read().decode())
        except HTTPError as e:
            error_body = e.read().decode() if e.fp else ""
            
            if e.code == 429:
                retry_after = int(e.headers.get("Retry-After", 60))
                raise RateLimitError(f"Rate limit exceeded: {error_body}", retry_after)
            elif e.code == 402:
                # Budget exceeded
                raise BudgetExceededError("unknown", 0, 0)
            elif e.code == 400:
                raise ValidationError("request", error_body)
            else:
                raise AiMeshError(f"HTTP {e.code}: {error_body}")
        except URLError as e:
            raise ConnectionError(f"Failed to connect to {url}: {e.reason}")
        except TimeoutError:
            raise TimeoutError(f"Request to {url} timed out")
    
    # Message Operations
    
    def send_message(self, message: Message) -> Acknowledgment:
        """
        Send a message for processing.
        
        Args:
            message: Message to send
            
        Returns:
            Acknowledgment with processing result
        """
        response = self._request("POST", "/messages", message.to_dict())
        return Acknowledgment.from_dict(response)
    
    def send_batch(self, messages: List[Message]) -> List[Acknowledgment]:
        """
        Send multiple messages in a batch.
        
        Args:
            messages: List of messages to send
            
        Returns:
            List of acknowledgments
        """
        data = {"messages": [m.to_dict() for m in messages]}
        response = self._request("POST", "/messages/batch", data)
        return [Acknowledgment.from_dict(ack) for ack in response.get("acknowledgments", [])]
    
    # Endpoint Operations
    
    def register_endpoint(self, metrics: EndpointMetrics) -> bool:
        """
        Register an AI endpoint.
        
        Args:
            metrics: Endpoint metrics
            
        Returns:
            True if successful
        """
        self._request("POST", "/endpoints", metrics.to_dict())
        return True
    
    def list_endpoints(self) -> List[EndpointMetrics]:
        """
        List all registered endpoints.
        
        Returns:
            List of endpoint metrics
        """
        response = self._request("GET", "/endpoints")
        return [
            EndpointMetrics(
                endpoint_id=e["endpoint_id"],
                capacity=e["capacity"],
                current_load=e["current_load"],
                cost_per_1k_tokens=e["cost_per_1k_tokens"],
                latency_p99_ms=e["latency_p99_ms"],
                error_rate=e["error_rate"],
                health_status=e.get("health_status", "healthy"),
            )
            for e in response.get("endpoints", [])
        ]
    
    def remove_endpoint(self, endpoint_id: str) -> bool:
        """
        Remove an endpoint.
        
        Args:
            endpoint_id: Endpoint to remove
            
        Returns:
            True if successful
        """
        self._request("DELETE", f"/endpoints/{endpoint_id}")
        return True
    
    # Budget Operations
    
    def set_budget(
        self,
        agent_id: str,
        tokens: float,
        reset_at: Optional[int] = None,
    ) -> bool:
        """
        Set token budget for an agent.
        
        Args:
            agent_id: Agent identifier
            tokens: Token budget
            reset_at: Optional reset timestamp (epoch ns)
            
        Returns:
            True if successful
        """
        data = {
            "agent_id": agent_id,
            "tokens": tokens,
            "reset_at": reset_at,
        }
        self._request("POST", "/budgets", data)
        return True
    
    def get_budget(self, agent_id: str) -> BudgetInfo:
        """
        Get budget info for an agent.
        
        Args:
            agent_id: Agent identifier
            
        Returns:
            Budget information
        """
        response = self._request("GET", f"/budgets/{agent_id}")
        return BudgetInfo.from_dict(response)
    
    def reset_budget(self, agent_id: str) -> bool:
        """
        Reset an agent's budget to initial value.
        
        Args:
            agent_id: Agent identifier
            
        Returns:
            True if successful
        """
        self._request("POST", f"/budgets/{agent_id}/reset")
        return True
    
    # Stats Operations
    
    def get_stats(self) -> dict:
        """
        Get system statistics.
        
        Returns:
            Statistics dictionary
        """
        return self._request("GET", "/stats")
    
    def get_metrics(self) -> str:
        """
        Get Prometheus metrics.
        
        Returns:
            Prometheus-formatted metrics string
        """
        url = f"{self.base_url}/metrics"
        req = Request(url, method="GET")
        
        try:
            with urlopen(req, timeout=self.timeout) as response:
                return response.read().decode()
        except Exception as e:
            raise AiMeshError(f"Failed to get metrics: {e}")
    
    def health_check(self) -> dict:
        """
        Check server health.
        
        Returns:
            Health status dictionary
        """
        return self._request("GET", "/health")
    
    # Convenience Methods
    
    def route_and_send(
        self,
        agent_id: str,
        payload: bytes,
        budget: float = 1000.0,
        priority: int = 50,
        metadata: Optional[Dict[str, str]] = None,
    ) -> Acknowledgment:
        """
        Convenience method to create and send a message.
        
        Args:
            agent_id: Agent identifier
            payload: Message payload
            budget: Token budget
            priority: Message priority (0-100)
            metadata: Optional metadata
            
        Returns:
            Acknowledgment
        """
        message = Message(
            agent_id=agent_id,
            payload=payload,
            budget_tokens=budget,
            priority=priority,
            metadata=metadata or {},
        )
        return self.send_message(message)
