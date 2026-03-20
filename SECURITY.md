# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest  | Yes       |
| Older   | No        |

Only the latest released version receives security fixes.

## Reporting a Vulnerability

Please report security vulnerabilities through [GitHub Security Advisories](https://github.com/Murzav/xcstrings-mcp/security/advisories).

**Do not open a public issue for security vulnerabilities.**

### Response Time

Security reports are handled on a best-effort basis. You can expect an initial acknowledgment within a few days.

## Scope

The following areas are in scope for security reports:

- **File parsing vulnerabilities** -- malformed `.xcstrings` input causing crashes, excessive memory use, or unexpected behavior
- **Path traversal** -- file paths escaping intended directories during read or write operations
- **Format string issues** -- format specifier handling leading to unexpected output or injection
