// Package aimesh provides a Go client for AiMesh AI agent message queue.
package aimesh

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	"github.com/google/uuid"
)

// Client is the AiMesh SDK client.
type Client struct {
	baseURL    string
	httpClient *http.Client
	apiKey     string
}

// ClientConfig configures the AiMesh client.
type ClientConfig struct {
	BaseURL string
	Timeout time.Duration
	APIKey  string
}

// NewClient creates a new AiMesh client.
func NewClient(config ClientConfig) *Client {
	if config.BaseURL == "" {
		config.BaseURL = "http://localhost:9000"
	}
	if config.Timeout == 0 {
		config.Timeout = 30 * time.Second
	}

	return &Client{
		baseURL: config.BaseURL,
		httpClient: &http.Client{
			Timeout: config.Timeout,
		},
		apiKey: config.APIKey,
	}
}

// Message represents an AI message.
type Message struct {
	AgentID            string            `json:"agent_id"`
	MessageID          string            `json:"message_id"`
	Payload            []byte            `json:"-"`
	PayloadHex         string            `json:"payload"`
	EstimatedCostToken float64           `json:"estimated_cost_tokens"`
	BudgetTokens       float64           `json:"budget_tokens"`
	DeadlineMs         int64             `json:"deadline_ms"`
	TaskGraphID        string            `json:"task_graph_id"`
	Dependencies       []string          `json:"dependencies"`
	Priority           int               `json:"priority"`
	DedupContext       string            `json:"dedup_context"`
	TraceID            string            `json:"trace_id"`
	Metadata           map[string]string `json:"metadata"`
	Timestamp          int64             `json:"timestamp"`
}

// NewMessage creates a new message.
func NewMessage(agentID string, payload []byte) *Message {
	return &Message{
		AgentID:      agentID,
		MessageID:    uuid.New().String(),
		Payload:      payload,
		PayloadHex:   hex.EncodeToString(payload),
		BudgetTokens: 1000,
		DeadlineMs:   time.Now().UnixMilli() + 60000,
		Priority:     50,
		Dependencies: []string{},
		Metadata:     make(map[string]string),
		Timestamp:    time.Now().UnixNano(),
	}
}

// Acknowledgment represents a processed message acknowledgment.
type Acknowledgment struct {
	OriginalMessageID   string  `json:"original_message_id"`
	Status              string  `json:"status"`
	TokensUsed          float64 `json:"tokens_used"`
	ProcessingLatencyMs int     `json:"processing_latency_ms"`
	Error               string  `json:"error"`
	Result              []byte  `json:"-"`
	ResultHex           string  `json:"result"`
}

// IsSuccess returns true if the message was processed successfully.
func (a *Acknowledgment) IsSuccess() bool {
	return a.Status == "success"
}

// EndpointMetrics represents AI endpoint metrics.
type EndpointMetrics struct {
	EndpointID      string  `json:"endpoint_id"`
	Capacity        int     `json:"capacity"`
	CurrentLoad     int     `json:"current_load"`
	CostPer1kTokens float64 `json:"cost_per_1k_tokens"`
	LatencyP99Ms    float64 `json:"latency_p99_ms"`
	ErrorRate       float64 `json:"error_rate"`
	HealthStatus    string  `json:"health_status"`
}

// BudgetInfo represents agent budget information.
type BudgetInfo struct {
	AgentID         string  `json:"agent_id"`
	InitialTokens   float64 `json:"initial_tokens"`
	RemainingTokens float64 `json:"remaining_tokens"`
	ConsumptionRate float64 `json:"consumption_rate"`
	ResetAt         int64   `json:"reset_at"`
}

// UtilizationPercent returns budget utilization percentage.
func (b *BudgetInfo) UtilizationPercent() float64 {
	if b.InitialTokens == 0 {
		return 0
	}
	return ((b.InitialTokens - b.RemainingTokens) / b.InitialTokens) * 100
}

// HealthStatus represents server health.
type HealthStatus struct {
	Status           string `json:"status"`
	UptimeSecs       int64  `json:"uptime_secs"`
	MessagesTotal    int64  `json:"messages_total"`
	EndpointsHealthy int    `json:"endpoints_healthy"`
	EndpointsTotal   int    `json:"endpoints_total"`
}

// Errors
var (
	ErrConnection     = fmt.Errorf("connection error")
	ErrRateLimit      = fmt.Errorf("rate limit exceeded")
	ErrBudgetExceeded = fmt.Errorf("budget exceeded")
	ErrValidation     = fmt.Errorf("validation error")
)

func (c *Client) request(method, path string, body interface{}) ([]byte, error) {
	var reqBody io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		reqBody = bytes.NewReader(data)
	}

	req, err := http.NewRequest(method, c.baseURL+path, reqBody)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrConnection, err)
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	if c.apiKey != "" {
		req.Header.Set("Authorization", "Bearer "+c.apiKey)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", ErrConnection, err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	switch resp.StatusCode {
	case 429:
		return nil, ErrRateLimit
	case 402:
		return nil, ErrBudgetExceeded
	case 400:
		return nil, fmt.Errorf("%w: %s", ErrValidation, string(respBody))
	}

	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("HTTP %d: %s", resp.StatusCode, string(respBody))
	}

	return respBody, nil
}

// SendMessage sends a message for processing.
func (c *Client) SendMessage(msg *Message) (*Acknowledgment, error) {
	data, err := c.request("POST", "/messages", msg)
	if err != nil {
		return nil, err
	}

	var ack Acknowledgment
	if err := json.Unmarshal(data, &ack); err != nil {
		return nil, err
	}

	if ack.ResultHex != "" {
		ack.Result, _ = hex.DecodeString(ack.ResultHex)
	}

	return &ack, nil
}

// RegisterEndpoint registers an AI endpoint.
func (c *Client) RegisterEndpoint(metrics *EndpointMetrics) error {
	_, err := c.request("POST", "/endpoints", metrics)
	return err
}

// ListEndpoints lists all registered endpoints.
func (c *Client) ListEndpoints() ([]EndpointMetrics, error) {
	data, err := c.request("GET", "/endpoints", nil)
	if err != nil {
		return nil, err
	}

	var resp struct {
		Endpoints []EndpointMetrics `json:"endpoints"`
	}
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, err
	}

	return resp.Endpoints, nil
}

// RemoveEndpoint removes an endpoint.
func (c *Client) RemoveEndpoint(endpointID string) error {
	_, err := c.request("DELETE", "/endpoints/"+endpointID, nil)
	return err
}

// SetBudget sets token budget for an agent.
func (c *Client) SetBudget(agentID string, tokens float64) error {
	_, err := c.request("POST", "/budgets", map[string]interface{}{
		"agent_id": agentID,
		"tokens":   tokens,
	})
	return err
}

// GetBudget gets budget info for an agent.
func (c *Client) GetBudget(agentID string) (*BudgetInfo, error) {
	data, err := c.request("GET", "/budgets/"+agentID, nil)
	if err != nil {
		return nil, err
	}

	var info BudgetInfo
	if err := json.Unmarshal(data, &info); err != nil {
		return nil, err
	}

	return &info, nil
}

// ResetBudget resets an agent's budget.
func (c *Client) ResetBudget(agentID string) error {
	_, err := c.request("POST", "/budgets/"+agentID+"/reset", nil)
	return err
}

// HealthCheck checks server health.
func (c *Client) HealthCheck() (*HealthStatus, error) {
	data, err := c.request("GET", "/health", nil)
	if err != nil {
		return nil, err
	}

	var status HealthStatus
	if err := json.Unmarshal(data, &status); err != nil {
		return nil, err
	}

	return &status, nil
}

// GetMetrics gets Prometheus metrics.
func (c *Client) GetMetrics() (string, error) {
	data, err := c.request("GET", "/metrics", nil)
	if err != nil {
		return "", err
	}
	return string(data), nil
}
