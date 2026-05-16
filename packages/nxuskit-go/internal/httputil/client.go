// Package httputil provides HTTP utilities for LLM provider implementations.
package httputil

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// DefaultTimeout is the default HTTP client timeout.
const DefaultTimeout = 60 * time.Second

// Client wraps an HTTP client with common functionality for LLM providers.
type Client struct {
	httpClient *http.Client
	baseURL    string
	headers    map[string]string
}

// NewClient creates a new HTTP client with the given base URL and timeout.
func NewClient(baseURL string, timeout time.Duration) *Client {
	return &Client{
		httpClient: &http.Client{
			Timeout: timeout,
		},
		baseURL: baseURL,
		headers: make(map[string]string),
	}
}

// NewClientWithHeaders creates a new HTTP client with default headers.
func NewClientWithHeaders(baseURL string, timeout time.Duration, headers map[string]string) *Client {
	h := make(map[string]string)
	for k, v := range headers {
		h[k] = v
	}
	return &Client{
		httpClient: &http.Client{
			Timeout: timeout,
		},
		baseURL: baseURL,
		headers: h,
	}
}

// SetHeader sets a default header that will be included in all requests.
func (c *Client) SetHeader(key, value string) {
	if c.headers == nil {
		c.headers = make(map[string]string)
	}
	c.headers[key] = value
}

// Do executes an HTTP request and returns the response.
// The caller is responsible for closing the response body.
func (c *Client) Do(req *http.Request) (*http.Response, error) {
	return c.httpClient.Do(req)
}

// PostJSON sends a POST request with JSON body and returns the response.
// The caller is responsible for closing the response body.
func (c *Client) PostJSON(ctx context.Context, path string, body any) (*http.Response, error) {
	return c.PostJSONWithHeaders(ctx, path, body, nil)
}

// PostJSONWithHeaders sends a POST request with JSON body and custom headers.
// Custom headers are merged with default headers, with custom headers taking precedence.
// The caller is responsible for closing the response body.
func (c *Client) PostJSONWithHeaders(ctx context.Context, path string, body any, headers map[string]string) (*http.Response, error) {
	data, err := json.Marshal(body)
	if err != nil {
		return nil, fmt.Errorf("marshal request body: %w", err)
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+path, bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}

	// Apply default headers
	for k, v := range c.headers {
		req.Header.Set(k, v)
	}

	// Apply custom headers (override defaults)
	for k, v := range headers {
		req.Header.Set(k, v)
	}

	req.Header.Set("Content-Type", "application/json")

	return c.httpClient.Do(req)
}

// Get sends a GET request and returns the response.
// The caller is responsible for closing the response body.
func (c *Client) Get(ctx context.Context, path string) (*http.Response, error) {
	return c.GetWithHeaders(ctx, path, nil)
}

// GetWithHeaders sends a GET request with custom headers.
// Custom headers are merged with default headers, with custom headers taking precedence.
// The caller is responsible for closing the response body.
func (c *Client) GetWithHeaders(ctx context.Context, path string, headers map[string]string) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+path, http.NoBody)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}

	// Apply default headers
	for k, v := range c.headers {
		req.Header.Set(k, v)
	}

	// Apply custom headers (override defaults)
	for k, v := range headers {
		req.Header.Set(k, v)
	}

	return c.httpClient.Do(req)
}

// BaseURL returns the client's base URL.
func (c *Client) BaseURL() string {
	return c.baseURL
}
