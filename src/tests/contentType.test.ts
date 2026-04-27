import { jest } from '@jest/globals'
import request from 'supertest'
import express from 'express'
import { requireJson, requireJsonForMethods } from '../middleware/requireJson.js'

describe('Content-Type Enforcement Middleware', () => {
  let app: express.Application

  beforeEach(() => {
    app = express()
    app.use(express.json())
    
    // Test endpoints
    app.post('/test', requireJson, (req, res) => {
      res.json({ success: true, body: req.body })
    })
    
    app.put('/test', requireJson, (req, res) => {
      res.json({ success: true, body: req.body })
    })
    
    app.patch('/test', requireJson, (req, res) => {
      res.json({ success: true, body: req.body })
    })
    
    app.delete('/test', requireJson, (req, res) => {
      res.json({ success: true, body: req.body })
    })
    
    app.get('/test', (req, res) => {
      res.json({ success: true })
    })
    
    app.head('/test', (req, res) => {
      res.status(200).end()
    })
    
    app.options('/test', (req, res) => {
      res.status(200).end()
    })
    
    // Test endpoint with method-specific enforcement
    app.post('/method-specific', requireJsonForMethods(['POST']), (req, res) => {
      res.json({ success: true, body: req.body })
    })
  })

  describe('POST requests', () => {
    test('should allow request with valid JSON content-type', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should allow request with JSON content-type and UTF-8 charset', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json; charset=utf-8')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should reject request without content-type header', async () => {
      const response = await request(app)
        .post('/test')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })
    })

    test('should reject request with invalid content-type', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'text/plain')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })
    })

    test('should reject request with non-UTF-8 charset', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json; charset=iso-8859-1')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Only UTF-8 charset is supported for JSON'
      })
    })

    test('should allow request with empty body', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json')
        .send()

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: undefined })
    })
  })

  describe('PUT requests', () => {
    test('should allow request with valid JSON content-type', async () => {
      const response = await request(app)
        .put('/test')
        .set('Content-Type', 'application/json')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should reject request without content-type header', async () => {
      const response = await request(app)
        .put('/test')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })
    })
  })

  describe('PATCH requests', () => {
    test('should allow request with valid JSON content-type', async () => {
      const response = await request(app)
        .patch('/test')
        .set('Content-Type', 'application/json')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should reject request with invalid content-type', async () => {
      const response = await request(app)
        .patch('/test')
        .set('Content-Type', 'text/html')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })
    })
  })

  describe('DELETE requests', () => {
    test('should allow request with valid JSON content-type', async () => {
      const response = await request(app)
        .delete('/test')
        .set('Content-Type', 'application/json')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should reject request without content-type header', async () => {
      const response = await request(app)
        .delete('/test')
        .send({ data: 'test' })

      expect(response.status).toBe(415)
      expect(response.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })
    })
  })

  describe('GET requests', () => {
    test('should allow GET requests without content-type enforcement', async () => {
      const response = await request(app)
        .get('/test')

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true })
    })
  })

  describe('HEAD requests', () => {
    test('should allow HEAD requests without content-type enforcement', async () => {
      const response = await request(app)
        .head('/test')

      expect(response.status).toBe(200)
    })
  })

  describe('OPTIONS requests', () => {
    test('should allow OPTIONS requests without content-type enforcement', async () => {
      const response = await request(app)
        .options('/test')

      expect(response.status).toBe(200)
    })
  })

  describe('requireJsonForMethods', () => {
    test('should enforce content-type for specified methods only', async () => {
      // Should enforce for POST
      const postResponse = await request(app)
        .post('/method-specific')
        .send({ data: 'test' })

      expect(postResponse.status).toBe(415)
      expect(postResponse.body).toEqual({
        error: 'Unsupported Media Type: Content-Type must be application/json'
      })

      // Should allow for GET (not in specified methods)
      const getResponse = await request(app)
        .get('/method-specific')

      expect(getResponse.status).toBe(404) // 404 because route doesn't exist for GET
    })

    test('should allow request with valid content-type for specified method', async () => {
      const response = await request(app)
        .post('/method-specific')
        .set('Content-Type', 'application/json')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })
  })

  describe('Edge cases', () => {
    test('should handle case-insensitive content-type header', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'APPLICATION/JSON')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should handle content-type with extra whitespace', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', '  application/json  ')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should handle content-type with additional parameters', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json; charset=utf-8; boundary=something')
        .send({ data: 'test' })

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: { data: 'test' } })
    })

    test('should allow requests with zero content-length', async () => {
      const response = await request(app)
        .post('/test')
        .set('Content-Type', 'application/json')
        .set('Content-Length', '0')
        .send()

      expect(response.status).toBe(200)
      expect(response.body).toEqual({ success: true, body: undefined })
    })
  })
})
