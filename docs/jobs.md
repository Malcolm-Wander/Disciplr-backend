# Jobs Enqueue Contract

`POST /api/jobs/enqueue` is admin-only and validates payload by job type using a discriminated schema.

## Supported job types

- `notification.send`
- `deadline.check`
- `oracle.call`
- `analytics.recompute`

## Enqueue options

- `delayMs`: optional, must be `>= 0`
- `maxAttempts`: optional integer, bounds `1..10`

Options parsing behavior:

- `delayMs` is floored before queue scheduling.
- `maxAttempts` is used as provided after schema validation.

## Retry failed jobs

`POST /api/admin/jobs/:id/retry` is an admin-only endpoint to retry a failed job.

- Resets a job's attempts to 0 and queues it for immediate execution.
- If the job has exhausted its `max_attempts` (i.e. is dead-lettered), the request will be refused unless `?force=true` is passed as a query parameter.
- Emits a `job.retry` audit log upon success.

## Error contract

Invalid payloads return:

- HTTP `400`
- `VALIDATION_ERROR` response body from `formatValidationError`
- field-level paths (for example `payload.scope`, `maxAttempts`, `delayMs`)

## Security

- Endpoint requires valid auth token and `ADMIN` role.
- Non-admin users receive `403`.
- On success, enqueue action writes `job.enqueue` audit logs.
