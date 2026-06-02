import type { NextFunction, Request, Response } from 'express'
import { logger, withCorrelationId, getOrGenerateCorrelationId } from './logger.js'

/**
 * Request logging middleware using Pino for structured JSON output.
 *
 * Emits a single JSON line per request with:
 * - Correlation ID (extracted from headers or generated)
 * - Request method, URL, query parameters
 * - Response status code and duration
 * - Request size (if available)
 * - User ID (if available from headers)
 *
 * All sensitive fields are automatically redacted by Pino's redact configuration.
 */
export const requestLogger = (req: Request, res: Response, next: NextFunction) => {
  const start = Date.now()
  const correlationId = getOrGenerateCorrelationId(req)
  const requestLogger = withCorrelationId(logger, correlationId)

  // Extract useful request metadata
  const method = req.method
  const url = req.originalUrl
  const path = req.path
  const queryString = req.url.includes('?') ? req.url.split('?')[1] : undefined
  const userId = req.headers['x-user-id'] as string | undefined
  const userRole = req.headers['x-user-role'] as string | undefined
  const contentLength = req.headers['content-length']

  // Store correlation ID and logger on request for downstream handlers
  ;(req as any).correlationId = correlationId
  ;(req as any).logger = requestLogger

  res.on('finish', () => {
    const durationMs = Date.now() - start
    const statusCode = res.statusCode

    // Determine log level based on status code
    const logLevel =
      statusCode >= 500 ? 'error' :
      statusCode >= 400 ? 'warn' :
      statusCode >= 200 ? 'info' :
      'debug'

    // Emit structured JSON log
    requestLogger[logLevel](
      {
        event: 'http.request',
        req: {
          method,
          url,
          path,
          queryString,
          headers: req.headers,
          body: req.body,
          contentLength: contentLength ? parseInt(contentLength, 10) : undefined,
          userId,
          userRole,
        },
        res: {
          statusCode,
          headers: res.getHeaders(),
        },
        durationMs,
      },
      `${method} ${path} ${statusCode} ${durationMs}ms`,
    )
  })

  next()
}

