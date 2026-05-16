package testutil

import (
	"net/http"
	"os"
	"testing"
	"time"

	"github.com/jarcoal/httpmock"
)

// SetupMockProvider is a convenience function to activate httpmock and register
// a basic responder for the given provider endpoint.
func SetupMockProvider(method, url string, statusCode int, response any) {
	httpmock.RegisterResponder(method, url,
		httpmock.NewJsonResponderOrPanic(statusCode, response))
}

// MockTimeoutResponder creates a responder that delays before responding,
// useful for testing timeout behavior.
func MockTimeoutResponder(delay time.Duration, response any) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		time.Sleep(delay)
		resp, err := httpmock.NewJsonResponse(200, response)
		return resp, err
	}
}

// MockTimeoutError creates a responder that delays and then returns an error,
// simulating a request timeout.
func MockTimeoutError(delay time.Duration) httpmock.Responder {
	return func(_ *http.Request) (*http.Response, error) {
		time.Sleep(delay)
		return nil, &timeoutError{message: "request timeout"}
	}
}

// timeoutError implements the net.Error interface for timeout simulation.
type timeoutError struct {
	message string
}

func (e *timeoutError) Error() string   { return e.message }
func (e *timeoutError) Timeout() bool   { return true }
func (e *timeoutError) Temporary() bool { return true }

// RequireEnvOrSkip checks if the specified environment variable is set.
// If not set, it skips the test with a message.
// Returns the value if set.
func RequireEnvOrSkip(t *testing.T, envVar string) string {
	t.Helper()
	value := os.Getenv(envVar)
	if value == "" {
		t.Skipf("Skipping test: %s environment variable not set", envVar)
	}
	return value
}

// RequireEnvsOrSkip checks if all specified environment variables are set.
// If any are missing, it skips the test with a message listing all missing vars.
// Returns a map of variable names to their values.
func RequireEnvsOrSkip(t *testing.T, envVars ...string) map[string]string {
	t.Helper()
	values := make(map[string]string)
	var missing []string

	for _, envVar := range envVars {
		value := os.Getenv(envVar)
		if value == "" {
			missing = append(missing, envVar)
		} else {
			values[envVar] = value
		}
	}

	if len(missing) > 0 {
		t.Skipf("Skipping test: missing environment variables: %v", missing)
	}

	return values
}

// ActivateMock is a convenience wrapper that activates httpmock and returns
// a cleanup function suitable for use with t.Cleanup or defer.
func ActivateMock() func() {
	httpmock.Activate()
	return httpmock.DeactivateAndReset
}

// AssertCallCount verifies that a specific endpoint was called the expected number of times.
func AssertCallCount(t *testing.T, method, url string, expected int) {
	t.Helper()
	info := httpmock.GetCallCountInfo()
	key := method + " " + url
	actual := info[key]
	if actual != expected {
		t.Errorf("Expected %d calls to %s, got %d", expected, key, actual)
	}
}

// AssertTotalCallCount verifies the total number of HTTP calls made.
func AssertTotalCallCount(t *testing.T, expected int) {
	t.Helper()
	actual := httpmock.GetTotalCallCount()
	if actual != expected {
		t.Errorf("Expected %d total HTTP calls, got %d", expected, actual)
	}
}

// GetCallCount returns the number of times a specific endpoint was called.
func GetCallCount(method, url string) int {
	info := httpmock.GetCallCountInfo()
	key := method + " " + url
	return info[key]
}

// IntPtr returns a pointer to an int value.
// Useful for setting optional int fields in test fixtures.
func IntPtr(v int) *int {
	return &v
}

// Float64Ptr returns a pointer to a float64 value.
// Useful for setting optional float64 fields in test fixtures.
func Float64Ptr(v float64) *float64 {
	return &v
}

// StringPtr returns a pointer to a string value.
// Useful for setting optional string fields in test fixtures.
func StringPtr(v string) *string {
	return &v
}

// BoolPtr returns a pointer to a bool value.
// Useful for setting optional bool fields in test fixtures.
func BoolPtr(v bool) *bool {
	return &v
}
