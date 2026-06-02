# Notification Delivery System

The application uses an abstraction for notification delivery, allowing for multiple providers (Email, Console, etc.) and reliable delivery via background jobs.

## Architecture

1.  **Job Enqueueing**: Notifications are enqueued as `notification.send` jobs.
2.  **Job Execution**: The job handler uses an injected `NotificationService` instance to select and execute the configured provider.
3.  **Retries**: Jobs are automatically retried with exponential backoff on failure.

## Provider Interface

All providers must implement the `NotificationProvider` interface:

```typescript
export interface NotificationProvider {
  name: string
  send(recipient: string, subject: string, body: string): Promise<void>
}
```

## Configuration

The active provider is selected at boot via the validated `NOTIFICATION_PROVIDER` environment variable and injected through `src/index.ts` -> `src/app-bootstrap.ts` -> `BackgroundJobSystem`.
Available providers:
- `email`: Sends via Email (Stub implementation).
- `console`: Logs to console (Default for local development).

### Fail-fast behavior

- `NotificationService` is initialized with a provider registry and a default provider name.
- If the configured provider name is unknown, startup fails with an explicit error.
- If a runtime override requests an unknown provider, the send operation throws immediately.
- Silent fallback to `console` for unknown provider names is intentionally removed to avoid masking misconfiguration.

## Observability

- **Metrics**: Queue metrics can be accessed via `GET /api/jobs/metrics`.
- **Logs**: Job execution is logged. PII (recipient, subject, body) is filtered from the logs for security and compliance.
- **Failures**: Persistent failures are recorded and observable via the metrics endpoint.

## Retry Policy

The system uses an exponential backoff strategy:
- `delay = min(60s, 1s * 2^(attempt - 1))`
- Execution is observable via `/api/jobs/metrics` and failures are tracked with their error messages.
