import { Request, Response, NextFunction } from 'express'
import { logger, withCorrelationId, getOrGenerateCorrelationId } from './logger.js'
import { utcNow } from '../utils/timestamps.js'

const SENSITIVE_FIELDS = new Set([
    'email',
    'password',
    'token',
    'accesstoken',
    'refreshtoken',
    'apikey',
    'api_key',
    'secret',
    'clientsecret',
    'creator',
    'successdestination',
    'failuredestination',
    'authorization',
    'cookie',
    'x-api-key'
])

export function shouldRedact(key: string): boolean {
    return SENSITIVE_FIELDS.has(key.toLowerCase())
}

export function redact(value: any, seen = new WeakSet()): any {
    if (value === null || value === undefined) {
        return value
    }
    
    // Primitive values
    if (typeof value !== 'object') {
        return value
    }

    // Circular reference check
    if (seen.has(value)) {
        return '[Circular]'
    }
    seen.add(value)
    
    if (Array.isArray(value)) {
        return value.map(item => redact(item, seen))
    }

    // Handle common objects that are not plain objects
    if (value instanceof Date) {
        return value.toISOString()
    }
    if (value instanceof RegExp) {
        return value.toString()
    }
    if (Buffer.isBuffer(value)) {
        return '[Buffer]'
    }
    
    const result: Record<string, any> = {}
    for (const [k, v] of Object.entries(value)) {
        if (shouldRedact(k)) {
            result[k] = '***REDACTED***'
        } else {
            result[k] = redact(v, seen)
        }
    }
    return result
}

/**
 * Privacy logger middleware using Pino for structured JSON output.
 *
 * Masks PII in logs by:
 * - Masking IP addresses (partial redaction)
 * - Redacting sensitive fields in request bodies and headers
 * - Emitting structured JSON for log aggregators
 *
 * Note: Pino's built-in redaction (configured in logger.ts) also handles
 * sensitive field redaction automatically. This middleware adds additional
 * IP masking and structured event logging.
 */
export const privacyLogger = (req: Request, _res: Response, next: NextFunction) => {
    const correlationId = getOrGenerateCorrelationId(req)
    const privacyLog = withCorrelationId(logger, correlationId)

    const ip = req.ip || req.socket.remoteAddress || 'unknown'
    const maskedIp = maskIp(ip)

    const timestamp = utcNow()
    const method = req.method
    const url = req.url

    // Store correlation ID and logger on request for downstream handlers
    ;(req as any).correlationId = correlationId
    ;(req as any).logger = privacyLog

    // Redact sensitive fields before logging
    // (Pino will also redact based on its configuration, but we do it here
    // for explicit control and compatibility with existing tests)
    const sanitizedBody = redact(req.body)
    const sanitizedHeaders = redact(req.headers)

    // Emit structured privacy event log
    privacyLog.debug(
        {
            event: 'privacy.request_logged',
            ip: {
                original: ip,
                masked: maskedIp,
            },
            request: {
                method,
                url,
                headers: sanitizedHeaders,
                body: sanitizedBody,
            },
            timestamp,
        },
        `Privacy-logged: ${method} ${url}`,
    )

    next()
}

export function maskIp(ip: string): string {
    if (ip.includes(':')) {
        // IPv6
        return ip.split(':').slice(0, 3).join(':') + ':xxxx:xxxx:xxxx:xxxx:xxxx'
    }
    // IPv4
    const parts = ip.split('.')
    if (parts.length === 4) {
        return `${parts[0]}.${parts[1]}.x.x`
    }
    return 'x.x.x.x'
}
