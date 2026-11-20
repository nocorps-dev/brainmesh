# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities responsibly via GitHub Security Advisories or by contacting the maintainers directly. Do not open public issues for undisclosed vulnerabilities.

## Security Guarantees in BrainMesh

- **Privacy by Default**: Knowledge defaults to `private` visibility.
- **Secret & PII Redaction**: `SecretRedactor` scrubs API keys, Bearer tokens, passwords, and email addresses prior to storage.
- **Prompt Injection Mitigation**: Retrieved knowledge includes an explicit `UNTRUSTED EXTERNAL KNOWLEDGE` safety notice.
- **Tenant Isolation**: Visibility (`private`, `workspace`, `organization`, `trusted_network`, `public`) is enforced on every read and recall path, and PostgreSQL RLS policies are included in `migrations/postgres/0001_initial_schema.sql`.
