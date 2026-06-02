# Privacy Logging Guidelines

## Overview
Disciplr is committed to protecting user data. To ensure that sensitive Personally Identifiable Information (PII) and credentials are never written to long-term storage via logs, we have implemented **Pino-based structured logging** with automatic redaction of sensitive fields.

## Architecture

### Structured JSON Logging with Pino
The backend now uses **Pino** for efficient, structured JSON logging that is:
- **Machine-readable**: Emits single-line JSON per log event for easy ingestion into log aggregators (Datadog, ELK, Grafana Loki, etc.)
- **Secure by default**: Sensitive fields are automatically redacted via Pino's `redact` configuration
- **Developer-friendly**: Pretty-printed output in development for readability
- **Zero-overhead**: Minimal performance impact compared to console logging

### Two-Layer Redaction Strategy
1. **Pino built-in redaction** (`src/middleware/logger.ts`): Automatically redacts fields matching configured paths
2. **Explicit redaction engine** (`src/middleware/privacy-logger.ts`): Additional `redact()` function for explicit control and backward compatibility

### Correlation IDs
All logs include correlation IDs (from `x-correlation-id` or `x-request-id` headers) for end-to-end request tracing:
```json
{
  "correlationId": "550e8400-e29b-41d4-a716-446655440000",
  "event": "http.request",
  "durationMs": 45
}
```

## Redaction Policy

### Automatic Redaction Paths
The following paths are automatically redacted by Pino (value replaced with `***REDACTED***`):

#### Request Fields
- `req.headers.authorization` — Bearer tokens, API keys in Authorization header
- `req.headers.cookie` — Session cookies
- `req.headers["x-api-key"]` — Custom API key headers
- `req.body.password` — User passwords
- `req.body.token` — Auth tokens in body
- `req.body.accessToken` — OAuth access tokens
- `req.body.refreshToken` — OAuth refresh tokens
- `req.body.apiKey` — API keys in body
- `req.body.api_key` — Alternate API key format
- `req.body.secret` — Generic secrets
- `req.body.clientSecret` — OAuth client secrets
- `req.body.creator` — Vault creator addresses
- `req.body.successDestination` — Vault success destination addresses
- `req.body.failureDestination` — Vault failure destination addresses
- `req.body.email` — User email addresses

#### Response Fields
- `res.headers.authorization` — Bearer tokens in responses
- `res.headers.cookie` — Response cookies
- `res.headers["x-api-key"]` — API key headers in responses

#### Error Fields
- `err.authorization`, `err.password`, `err.token`, `err.apiKey`, `err.secret`

#### Metadata Fields
- All `metadata.*` sensitive fields (authorization, password, token, etc.)

#### Entity Fields
- `user.email`, `user.password`, `user.apiKey`
- `vault.creator`, `vault.successDestination`, `vault.failureDestination`

### Supported Data Structures
The redaction engine is recursive and works safely across:
- **Nested objects** — Redacts fields at any depth
- **Arrays** — Redacts sensitive fields in array elements
- **Standard objects** — Date, RegExp, Buffer objects are safely serialized
- **Circular references** — Protected against stack overflow

## Middleware Components

### Request Logger (`src/middleware/requestLogger.ts`)
Emits structured JSON for every HTTP request:
```json
{
  "correlationId": "550e8400-e29b-41d4-a716-446655440000",
  "event": "http.request",
  "req": {
    "method": "POST",
    "url": "/api/vaults",
    "path": "/api/vaults",
    "headers": { "authorization": "***REDACTED***" },
    "body": { "email": "***REDACTED***", "amount": 1000 },
    "userId": "user123",
    "userRole": "admin"
  },
  "res": { "statusCode": 201 },
  "durationMs": 45,
  "msg": "POST /api/vaults 201 45ms"
}
```

**Log Level Selection**:
- `error` (5xx status codes)
- `warn` (4xx status codes)
- `info` (2xx status codes)
- `debug` (1xx status codes)

### Privacy Logger (`src/middleware/privacy-logger.ts`)
Emits privacy-focused events with IP masking:
```json
{
  "correlationId": "550e8400-e29b-41d4-a716-446655440000",
  "event": "privacy.request_logged",
  "ip": {
    "original": "192.168.1.1",
    "masked": "192.168.x.x"
  },
  "request": {
    "method": "POST",
    "url": "/api/test",
    "headers": { "authorization": "***REDACTED***" },
    "body": { "email": "***REDACTED***" }
  },
  "timestamp": "2025-06-02T14:32:10.000Z",
  "msg": "Privacy-logged: POST /api/test"
}
```

**IP Masking**:
- IPv4: `192.168.1.1` → `192.168.x.x` (mask last 2 octets)
- IPv6: `2001:0db8:85a3::` → `2001:0db8:85a3:xxxx:xxxx:xxxx:xxxx:xxxx` (mask last 5 groups)

## Configuration

### Logger Setup (`src/middleware/logger.ts`)
```typescript
export const logger = createLogger()
```

**Environment Variables**:
- `NODE_ENV` — Enables pretty-printing in `development` mode
- `LOG_LEVEL` — Set minimum log level (`debug`, `info`, `warn`, `error`; default: `info`)

### In Development
Logs are pretty-printed with colors and indentation for readability:
```
 INFO  (disciplr-backend): POST /api/vaults 201 45ms
    req: {
      "method": "POST",
      "url": "/api/vaults"
    }
```

### In Production
Logs are emitted as single-line JSON:
```
{"correlationId":"550e8400...","event":"http.request","req":{...},"res":{"statusCode":201},"durationMs":45}
```

## Integration with Log Aggregators

### Example: Datadog
```bash
# Install Datadog agent on your infrastructure
# Provide Datadog API key

# Datadog will automatically ingest JSON logs and parse fields:
service: disciplr-backend
correlationId: 550e8400-e29b-41d4-a716-446655440000
event: http.request
req.method: POST
```

### Example: Grafana Loki
Configure Promtail to scrape and parse JSON:
```yaml
scrape_configs:
  - job_name: disciplr-backend
    static_configs:
      - targets:
          - localhost
        labels:
          job: disciplr-backend
    relabel_configs:
      - source_labels: [__address__]
        target_label: __param_target
```

### Example: ELK Stack
Logstash will parse JSON automatically:
```json
{
  "correlationId": "550e8400-e29b-41d4-a716-446655440000",
  "event": "http.request",
  "req.method": "POST"
}
```

## Adding New Redactions

### Option 1: Add to Pino Redact Paths (Automatic)
Edit `src/middleware/logger.ts` and add the path to the `redact.paths` array:
```typescript
redact: {
  paths: [
    'req.body.newSensitiveField',  // Add here
    // ...existing paths
  ]
}
```

### Option 2: Add to Explicit Redaction List (Backward Compat)
Edit `src/middleware/privacy-logger.ts` and add the field key to `SENSITIVE_FIELDS`:
```typescript
const SENSITIVE_FIELDS = new Set([
    'email',
    'password',
    'newSensitiveField',  // Add here
    // ...existing fields
])
```

## Accessing Logs in Downstream Handlers

Request handlers can use the injected logger for consistent structured logging:
```typescript
import { Request, Response, NextFunction } from 'express'

export const myHandler = (req: Request, res: Response, next: NextFunction) => {
  const logger = (req as any).logger
  const correlationId = (req as any).correlationId

  logger.info({ event: 'my_event', data: {...} }, 'Processing request')
  
  res.json({ message: 'Success' })
}
```

All logs from the same request will automatically share the correlation ID.

## Development vs Production

Redaction runs in **all environments** (development, staging, production) to:
- Prevent accidental ingestion of PII into development databases or logs
- Ensure parity in testing across environments
- Maintain security posture uniformly

Debugging should rely on non-sensitive identifiers:
- User IDs: `user123` (visible)
- Vault IDs: `vault456` (visible)
- Transaction references: `tx789` (visible)
- Email addresses: `***REDACTED***` (hidden)
- API keys: `***REDACTED***` (hidden)

## Testing

Run privacy logger tests to verify redaction coverage:
```bash
npm test -- src/tests/privacy-logger.test.ts
```

Coverage includes:
- ✅ Sensitive field redaction at all nesting levels
- ✅ IP masking (IPv4 and IPv6)
- ✅ Circular reference protection
- ✅ Date, RegExp, Buffer serialization
- ✅ Pino JSON structure verification
- ✅ PII leakage regression tests

## Compliance

This logging architecture supports compliance with:
- **GDPR** — Redaction prevents PII leakage to log storage
- **HIPAA** — Sensitive fields are never stored in unencrypted logs
- **SOC 2** — Structured logging enables audit trail generation
- **PCI DSS** — Passwords, tokens, and API keys are redacted