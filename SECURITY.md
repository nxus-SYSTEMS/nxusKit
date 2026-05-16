# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.9.x   | :white_check_mark: |
| < 0.9   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please report it responsibly.

### How to Report

1. **Do NOT** create a public GitHub issue for security vulnerabilities
2. Use the **"Report a security vulnerability"** issue template on GitHub
   (this creates a private security advisory visible only to maintainers)
3. Include as much detail as possible:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Any suggested fixes

### What to Expect

- **Acknowledgment**: We will acknowledge receipt within 48 hours
- **Initial Assessment**: Within 7 days, we will provide an initial assessment
- **Updates**: We will keep you informed of our progress
- **Resolution**: We aim to resolve critical issues within 30 days
- **Credit**: We will credit you in the security advisory (unless you prefer anonymity)

### Scope

This security policy covers:

- The `nxuskit-engine` library crate (nxuskit-core C ABI layer)
- The `nxuskit` Rust wrapper crate
- The `nxuskit-go` Go SDK
- The `nxuskit-py` Python SDK
- The `nxuskit-cli` binary crate
- The licensing and authentication infrastructure
- Official examples and documentation

### Out of Scope

- Third-party dependencies (report to their maintainers)
- LLM provider APIs (report to respective providers)
- Issues in user code that uses this library

## Security Considerations

### API Key Handling

nxusKit handles API keys for various LLM providers. Best practices:

- **Never** commit API keys to version control
- Use environment variables for API keys
- The library does not log or persist API keys
- API keys are only sent to their respective provider endpoints

### Network Security

- All API calls use HTTPS
- Certificate validation is enforced by default
- No sensitive data is logged at default log levels

### Dependencies

- Dependencies are regularly audited using `cargo audit`
- We minimize dependencies to reduce attack surface
- All dependencies are from crates.io with verified publishers where possible

## Security Best Practices for Users

1. **Environment Variables**: Store API keys in environment variables, not in code
2. **Minimal Permissions**: Use API keys with minimal required permissions
3. **Key Rotation**: Rotate API keys regularly
4. **Logging**: Be careful not to log request/response content containing sensitive data
5. **Input Validation**: Validate and sanitize user inputs before sending to LLMs

## Known Security Considerations

### Prompt Injection

LLMs are susceptible to prompt injection attacks. This library does not provide protection against prompt injection - that is the responsibility of the application developer. Consider:

- Validating and sanitizing user inputs
- Using system prompts to establish boundaries
- Not blindly executing LLM outputs

### Data Privacy

Content sent to LLM providers is processed according to each provider's terms of service and privacy policy. Ensure compliance with your data handling requirements.

## Acknowledgments

We thank the security researchers who have helped improve the security of this project:

- (No reports yet)
