# Makefile for nxuskit-go
# Run `make help` for available targets

.PHONY: all build test test-race test-coverage lint fmt vet check clean help security tools deps tidy doc

# Default target
all: check

# Build the package (library only, no binary output)
build:
	@echo "Building library..."
	go build -v ./...

# Run tests
test:
	@echo "Running tests..."
	go test -v ./...

# Run tests with race detector
test-race:
	@echo "Running tests with race detector..."
	go test -race -v ./...

# Run tests with coverage
test-coverage:
	@echo "Running tests with coverage..."
	go test -race -coverprofile=coverage.out -covermode=atomic ./...
	go tool cover -func=coverage.out
	@echo ""
	@echo "Coverage report generated: coverage.out"
	@echo "Run 'go tool cover -html=coverage.out' to view in browser"

# Check coverage meets 90% threshold
coverage-check: test-coverage
	@echo "Checking coverage threshold..."
	@COVERAGE=$$(go tool cover -func=coverage.out | grep total | awk '{print substr($$3, 1, length($$3)-1)}'); \
	echo "Total coverage: $${COVERAGE}%"; \
	if [ $$(echo "$$COVERAGE < 90" | bc -l) -eq 1 ]; then \
		echo "ERROR: Coverage $${COVERAGE}% is below 90% threshold"; \
		exit 1; \
	else \
		echo "OK: Coverage meets 90% threshold"; \
	fi

# Run linter
lint:
	@echo "Running linter..."
	golangci-lint run --timeout=5m

# Format code
fmt:
	@echo "Formatting code..."
	gofmt -s -w .
	goimports -w -local github.com/nxus-SYSTEMS/nxusKit/nxuskit-go .

# Run go vet
vet:
	@echo "Running go vet..."
	go vet ./...

# Run security audit
security:
	@echo "Running security audit..."
	govulncheck ./...

# Install development tools
tools:
	@echo "Installing development tools..."
	go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest
	go install golang.org/x/tools/cmd/goimports@latest
	go install golang.org/x/vuln/cmd/govulncheck@latest
	@echo "Tools installed successfully"

# Run all checks (fmt, vet, lint, test)
check: fmt vet lint test-race
	@echo ""
	@echo "All checks passed!"

# Clean build artifacts
clean:
	@echo "Cleaning..."
	go clean -cache -testcache
	rm -f coverage.out coverage.html
	rm -rf bin/

# Download dependencies
deps:
	@echo "Downloading dependencies..."
	go mod download
	go mod verify

# Tidy dependencies
tidy:
	@echo "Tidying dependencies..."
	go mod tidy

# Generate documentation
doc:
	@echo "Starting godoc server at http://localhost:6060/pkg/github.com/nxus-SYSTEMS/nxusKit/nxuskit-go/"
	godoc -http=:6060

# Show help
help:
	@echo "nxuskit-go Makefile"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  all            Run all checks (default)"
	@echo "  build          Build the package (library)"
	@echo "  test           Run tests"
	@echo "  test-race      Run tests with race detector"
	@echo "  test-coverage  Run tests with coverage report"
	@echo "  coverage-check Check coverage meets 90% threshold"
	@echo "  lint           Run golangci-lint"
	@echo "  fmt            Format code with gofmt and goimports"
	@echo "  vet            Run go vet"
	@echo "  security       Run govulncheck security audit"
	@echo "  tools          Install development tools"
	@echo "  check          Run all checks (fmt, vet, lint, test)"
	@echo "  clean          Clean build artifacts"
	@echo "  deps           Download dependencies"
	@echo "  tidy           Tidy go.mod"
	@echo "  doc            Start godoc server"
	@echo "  help           Show this help"
