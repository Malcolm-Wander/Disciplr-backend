import { initEnv } from './config/index.js'

// Validate environment variables before any other initialisation.
// This ensures the process exits immediately on misconfiguration.
initEnv()

import { ensureSorobanBootPrecheck } from './services/sorobanBoot.js'

import { app } from './app.js'
import { bootstrapApp } from './app-bootstrap.js'
import { startExpirationChecker } from './services/expirationScheduler.js'
import { orgVaultsRouter } from './routes/orgVaults.js'
import { orgAnalyticsRouter } from './routes/orgAnalytics.js'
import { orgMembersRouter } from './routes/orgMembers.js'
import { adminRouter } from './routes/admin.js'
import { adminVerifiersRouter } from './routes/adminVerifiers.js'
import { verificationsRouter } from './routes/verifications.js'
import { apiKeysRouter } from './routes/apiKeys.js'
import { notificationsRouter } from './routes/notifications.js'
import {
  securityMetricsMiddleware,
  securityRateLimitMiddleware,
} from './security/abuse-monitor.js'
import { initializeDatabase, closeDatabase } from './db/database.js'
import { etlWorker } from './services/etlWorker.js'
import { createShutdownHandler } from './server/shutdown.js'

const PORT = process.env.PORT ?? 3000

// Initialize SQLite database for analytics
initializeDatabase()

const { jobSystem } = bootstrapApp()

jobSystem.start()

const ETL_INTERVAL_MINUTES = parseInt(process.env.ETL_INTERVAL_MINUTES ?? '5', 10)

const server = app.listen(PORT, () => {
  console.log(`Disciplr API listening on http://localhost:${PORT}`)
  startExpirationChecker()
  if (process.env.ENABLE_ETL_WORKER !== 'false') {
    etlWorker.start(ETL_INTERVAL_MINUTES)
  }
  void ensureSorobanBootPrecheck()
})

const shutdownHandler = createShutdownHandler({
  server,
  jobSystem,
  etlWorker,
  closeDb: closeDatabase,
})

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.on(signal, () => {
    void shutdownHandler(signal)
  })
}
