import { jest } from '@jest/globals'
import { Request, Response, NextFunction } from 'express'
import { privacyLogger, redact, maskIp, shouldRedact } from '../middleware/privacy-logger.js'
import * as loggerModule from '../middleware/logger.js'

// Mock the logger module
jest.mock('../middleware/logger.js', () => ({
    logger: {
        debug: jest.fn(),
        info: jest.fn(),
        warn: jest.fn(),
        error: jest.fn(),
        child: jest.fn(function(this: any) {
            return this
        }),
    },
    withCorrelationId: jest.fn((log, cid) => ({
        ...log,
        child: jest.fn(function() {
            return this
        }),
        debug: jest.fn(),
        info: jest.fn(),
        warn: jest.fn(),
        error: jest.fn(),
    })),
    getOrGenerateCorrelationId: jest.fn((req) => req.headers['x-correlation-id'] || 'default-cid'),
}))

describe('Privacy Logger', () => {
    describe('Redaction Engine', () => {
        it('should redact sensitive fields at the top level', () => {
            const payload = {
                email: 'test@example.com',
                userId: '12345',
                token: 'super-secret-token'
            }
            const expected = {
                email: '***REDACTED***',
                userId: '12345',
                token: '***REDACTED***'
            }
            expect(redact(payload)).toEqual(expected)
        })

        it('should redact sensitive fields in nested objects', () => {
            const payload = {
                user: {
                    name: 'John Doe',
                    email: 'john@example.com',
                    auth: {
                        apiKey: 'test-api-key',
                        scopes: ['read']
                    }
                }
            }
            const expected = {
                user: {
                    name: 'John Doe',
                    email: '***REDACTED***',
                    auth: {
                        apiKey: '***REDACTED***',
                        scopes: ['read']
                    }
                }
            }
            expect(redact(payload)).toEqual(expected)
        })

        it('should redact sensitive fields within arrays', () => {
            const payload = {
                users: [
                    { id: '1', email: 'test1@example.com' },
                    { id: '2', email: 'test2@example.com' }
                ],
                tags: ['a', 'b']
            }
            const expected = {
                users: [
                    { id: '1', email: '***REDACTED***' },
                    { id: '2', email: '***REDACTED***' }
                ],
                tags: ['a', 'b']
            }
            expect(redact(payload)).toEqual(expected)
        })

        it('should handle nulls, undefined, and non-object values', () => {
            expect(redact(null)).toBeNull()
            expect(redact(undefined)).toBeUndefined()
            expect(redact('string')).toBe('string')
            expect(redact(123)).toBe(123)
        })
        
        it('should redact fields case-insensitively', () => {
            expect(redact({ EMAIL: 'test@test.com' })).toEqual({ EMAIL: '***REDACTED***' })
            expect(redact({ ApiKey: 'key' })).toEqual({ ApiKey: '***REDACTED***' })
        })
        
        it('should verify behavior of shouldRedact directly', () => {
            expect(shouldRedact('token')).toBe(true)
            expect(shouldRedact('userId')).toBe(false)
        })
        it('should handle Date, RegExp, and Buffer objects correctly', () => {
            const date = new Date('2025-01-01T00:00:00Z')
            const regex = /test/i
            const buffer = Buffer.from('test')

            const payload = {
                date,
                regex,
                buffer,
                email: 'test@example.com'
            }
            
            const expected = {
                date: date.toISOString(),
                regex: regex.toString(),
                buffer: '[Buffer]',
                email: '***REDACTED***'
            }
            expect(redact(payload)).toEqual(expected)
        })

        it('should handle circular references without stack overflow', () => {
            const payload: any = {
                email: 'test@example.com'
            }
            payload.self = payload
            
            const expected = {
                email: '***REDACTED***',
                self: '[Circular]'
            }
            expect(redact(payload)).toEqual(expected)
        })
    })

    describe('IP Masking', () => {
        it('should mask IPv4 address', () => {
            expect(maskIp('192.168.1.1')).toBe('192.168.x.x')
        })

        it('should handle unknown or malformed IPv4 gracefully', () => {
            expect(maskIp('192')).toBe('x.x.x.x')
        })

        it('should mask IPv6 address', () => {
            expect(maskIp('2001:0db8:85a3:0000:0000:8a2e:0370:7334')).toBe('2001:0db8:85a3:xxxx:xxxx:xxxx:xxxx:xxxx')
        })
    })

    describe('Express Middleware integration with Pino', () => {
        let req: Partial<Request>
        let res: Partial<Response>
        let next: NextFunction
        let mockLogger: any

        beforeEach(() => {
            jest.clearAllMocks()

            mockLogger = {
                debug: jest.fn(),
                info: jest.fn(),
                warn: jest.fn(),
                error: jest.fn(),
            }

            req = {
                ip: '192.168.0.1',
                method: 'POST',
                url: '/api/test',
                body: { email: 'user@example.com', name: 'Bob' },
                headers: { 
                    authorization: 'Bearer 1234', 
                    'user-agent': 'jest',
                    'x-correlation-id': 'test-corr-id'
                },
                socket: {} as any
            }
            res = {}
            next = jest.fn()

            // Mock withCorrelationId to return our mock logger
            ;(loggerModule.withCorrelationId as jest.Mock).mockReturnValue(mockLogger)
        })

        afterEach(() => {
            jest.restoreAllMocks()
        })

        it('should call privacyLogger and invoke next', () => {
            privacyLogger(req as Request, res as Response, next)
            expect(next).toHaveBeenCalled()
        })

        it('should emit structured JSON log event through pino', () => {
            privacyLogger(req as Request, res as Response, next)
            
            expect(mockLogger.debug).toHaveBeenCalled()
            const callArgs = mockLogger.debug.mock.calls[0]
            
            // First argument should be the structured log object
            const logObject = callArgs[0]
            expect(logObject.event).toBe('privacy.request_logged')
            expect(logObject.ip.original).toBe('192.168.0.1')
            expect(logObject.ip.masked).toBe('192.168.x.x')
            expect(logObject.request.method).toBe('POST')
            expect(logObject.request.url).toBe('/api/test')
        })

        it('should redact sensitive fields in structured log output', () => {
            privacyLogger(req as Request, res as Response, next)
            
            const logObject = mockLogger.debug.mock.calls[0][0]
            
            // Body should have redacted email
            expect(logObject.request.body.email).toBe('***REDACTED***')
            expect(logObject.request.body.name).toBe('Bob')
            
            // Headers should have redacted authorization
            expect(logObject.request.headers.authorization).toBe('***REDACTED***')
            expect(logObject.request.headers['user-agent']).toBe('jest')
        })

        it('should emit human-readable message as second argument', () => {
            privacyLogger(req as Request, res as Response, next)
            
            const callArgs = mockLogger.debug.mock.calls[0]
            const message = callArgs[1]
            
            expect(message).toContain('Privacy-logged')
            expect(message).toContain('POST')
            expect(message).toContain('/api/test')
        })

        it('should handle correlation IDs from request headers', () => {
            privacyLogger(req as Request, res as Response, next)
            
            expect(loggerModule.getOrGenerateCorrelationId).toHaveBeenCalledWith(req)
        })

        it('should ensure regression against PII leakage in structured logs', () => {
            req.body = { 
                apiKey: 'super_secret_key_123', 
                nested: { token: 'hidden_token', user_email: 'safe@test.com' } 
            }
            req.headers = { 'x-api-key': 'header_secret_key' }
            
            privacyLogger(req as Request, res as Response, next)
            
            const logObject = mockLogger.debug.mock.calls[0][0]
            const logString = JSON.stringify(logObject)
            
            // Regression test: absolutely no sensitive strings should be present
            expect(logString).not.toMatch(/super_secret_key_123/)
            expect(logString).not.toMatch(/hidden_token/)
            expect(logString).not.toMatch(/header_secret_key/)
            expect(logString).toContain('***REDACTED***')
        })

        it('should attach correlation ID and logger to request for downstream handlers', () => {
            privacyLogger(req as Request, res as Response, next)
            
            expect((req as any).correlationId).toBe('test-corr-id')
            expect((req as any).logger).toBeDefined()
        })
    })

    describe('Structured JSON output format', () => {
        let req: Partial<Request>
        let res: Partial<Response>
        let next: NextFunction
        let mockLogger: any

        beforeEach(() => {
            jest.clearAllMocks()

            mockLogger = {
                debug: jest.fn(),
                info: jest.fn(),
                warn: jest.fn(),
                error: jest.fn(),
            }

            req = {
                ip: '10.0.0.5',
                method: 'DELETE',
                url: '/api/vaults/123',
                body: {},
                headers: { 'user-id': '456' },
                socket: {} as any
            }
            res = {}
            next = jest.fn()

            ;(loggerModule.withCorrelationId as jest.Mock).mockReturnValue(mockLogger)
        })

        it('should emit complete and valid JSON structure', () => {
            privacyLogger(req as Request, res as Response, next)
            
            const logObject = mockLogger.debug.mock.calls[0][0]
            
            // Verify all required fields are present
            expect(logObject).toHaveProperty('event')
            expect(logObject).toHaveProperty('ip')
            expect(logObject).toHaveProperty('request')
            expect(logObject).toHaveProperty('timestamp')
            
            // Verify nested structures
            expect(logObject.ip).toHaveProperty('original')
            expect(logObject.ip).toHaveProperty('masked')
            expect(logObject.request).toHaveProperty('method')
            expect(logObject.request).toHaveProperty('url')
            expect(logObject.request).toHaveProperty('headers')
            expect(logObject.request).toHaveProperty('body')
            
            // Verify JSON serializability
            expect(() => JSON.stringify(logObject)).not.toThrow()
        })
    })
})

