# Contributing to nxusKit

Thank you for your interest in contributing to nxusKit! This document provides guidelines and information for contributors.

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before contributing.

## Project Overview

nxusKit is a polyglot LLM toolkit with implementations in multiple languages:
- **nxuskit-engine** (Rust) - Reference implementation
- **nxuskit-go** (Go) - Cloud-native focus
- **nxuskit-py** (Python) - Data science focus (coming soon)
- **gateway** (TypeScript/Node) - API gateway (coming soon)

## Repository Structure

```
nxusKit/
├── packages/
│   ├── nxuskit-engine/      # Rust implementation
│   ├── nxuskit-go/      # Go implementation
│   ├── nxuskit-py/     # Python implementation (future)
│   └── gateway/       # Node.js gateway (future)
├── conformance/       # Cross-language test vectors
├── docs/              # Documentation
└── tools/             # Development utilities
```

## Contribution Workflow

### For External Contributors (Fork & PR)

nxusKit uses an **internal-first** contribution workflow:

1. **Fork** the public repository (`nxus-SYSTEMS/nxusKit`)
2. **Create a branch** for your changes
3. **Make your changes** following our guidelines
4. **Submit a Pull Request** to the public repository
5. **CI runs** and provides feedback on your PR
6. **Maintainer reviews** your contribution
7. **If approved**, changes are replayed to the internal repository
8. **Changes appear** in the public repo after the next publish cycle

Your contribution will be credited in the commit history.

### What Happens to Your PR

- PRs are reviewed on the public repository
- Approved changes are applied to the internal repository first
- The publish workflow then mirrors changes back to public
- Your original PR may be closed as "merged via internal" once changes appear

## Beta Status Notice

nxusKit is currently in **beta** (v0.x.x). While the API is stabilizing, breaking changes may still occur. We especially welcome:

- Bug reports and fixes
- Documentation improvements
- Test coverage improvements
- Provider implementations
- Performance optimizations

## Project Constitution

**CRITICAL**: This project is governed by a Constitution that defines non-negotiable principles. All contributions MUST adhere to these principles.

### Key Principles

1. **Test-Driven Development**: Tests MUST be written before implementation
2. **Type Safety**: Leverage each language's type system
3. **Provider Abstraction**: Providers must be isolated and interchangeable
4. **Async-First**: All I/O operations must be async
5. **Minimal Dependencies**: Every dependency must be justified
6. **Cross-Language Consistency**: API consistency across implementations

## How to Contribute

### Reporting Bugs

Before creating a bug report, please check existing issues to avoid duplicates. When creating a bug report, include:

- **Language/version**: Which implementation (nxuskit-engine, nxuskit-go, etc.) and version
- **Operating system**: Platform and version
- **Steps to reproduce**: Minimal code example if possible
- **Expected behavior**: What you expected to happen
- **Actual behavior**: What actually happened
- **Error messages**: Full error output, including backtraces

### Suggesting Features

Feature suggestions are welcome! Please:

1. Check existing issues and discussions first
2. Describe the use case and problem you're trying to solve
3. Explain why existing functionality doesn't meet your needs
4. If possible, sketch out how the API might look
5. Consider how the feature would work across all language implementations

### Pull Requests

1. **Fork the repository** and create your branch from `main`
2. **Follow the coding style** for the relevant language
3. **Add tests** for any new functionality
4. **Update documentation** as needed
5. **Run the test suite** before submitting
6. **Write a clear PR description** explaining what and why

---

## Contributing to Rust (nxuskit-engine)

### Prerequisites

- Rust 1.92 or later
- Cargo (comes with Rust)

### Setup

```bash
git clone https://github.com/nxus-SYSTEMS/nxusKit.git
cd nxusKit

# Build the Rust library
cargo build -p nxuskit-engine

# Run tests
cargo test -p nxuskit-engine
```

### Code Quality Checklist

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --all-features -- -D warnings

# Run all tests
cargo test --all-features

# Check documentation
cargo doc --no-deps
```

---

## Contributing to Go (nxuskit-go)

### Prerequisites

- Go 1.22 or later

### Setup

```bash
git clone https://github.com/nxus-SYSTEMS/nxusKit.git
cd nxusKit/packages/nxuskit-go

# Build
go build ./...

# Run tests
go test ./...
```

### Code Quality Checklist

```bash
# Format code
gofmt -w .

# Run linter
golangci-lint run ./...

# Run tests with coverage
go test -race -coverprofile=coverage.out ./...
go tool cover -func=coverage.out
```

---

## Coding Standards

### All Languages

- Follow language-idiomatic conventions
- Document all public APIs
- Write tests before implementation (TDD)
- Handle errors explicitly
- Keep dependencies minimal

### Rust Specific

- Use `rustfmt` for formatting
- Pass `clippy` with no warnings
- Follow Rust naming conventions
- Use the crate's `Result` type alias

### Go Specific

- Use `gofmt` and `goimports` for formatting
- Pass `golangci-lint` with no warnings
- Follow Go naming conventions
- Return `(T, error)` for fallible operations

## Commit Messages

Use conventional commit format:

```
feat(claude): add support for Claude 3.5
fix(openai): correct token counting
docs: update README with new examples
test: add integration tests for streaming
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`

## Pull Request Checklist

- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] CHANGELOG.md updated (if applicable)
- [ ] Follows constitution principles
- [ ] Linter passes with no warnings
- [ ] Code formatted
- [ ] Cross-language impact considered

## Review Process

1. All PRs require at least one review
2. CI must pass (tests, linting, formatting)
3. Documentation must be updated if needed
4. Breaking changes require discussion first

## Getting Help

- Open a GitHub Discussion for questions
- Check existing issues and discussions
- Read the documentation in `docs/`

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project (MIT OR Apache-2.0).

Thank you for contributing!
