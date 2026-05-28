import type { CreateVaultInput, PersistedVault, VaultCreateResponse } from '../types/vaults.js'
import { retryWithBackoff, sleep, type RetryConfig } from '../utils/retry.js'

const DEFAULT_CONTRACT_ID = 'CONTRACT_ID_NOT_CONFIGURED'
const DEFAULT_SOURCE_ACCOUNT = 'SOURCE_ACCOUNT_NOT_CONFIGURED'
const DEFAULT_SUBMIT_POLL_INTERVAL_MS = 1000
const DEFAULT_SUBMIT_POLL_MAX_ATTEMPTS = 30
const DEFAULT_RPC_TIMEOUT_MS = 30_000
const DEFAULT_SUBMIT_RETRY_MAX_ATTEMPTS = 3
const DEFAULT_SUBMIT_RETRY_BACKOFF_MS = 100
const DEFAULT_SUBMIT_RETRY_MAX_BACKOFF_MS = 5_000
const DEFAULT_SUBMIT_RETRY_BACKOFF_MULTIPLIER = 2
const DEFAULT_SUBMIT_RETRY_JITTER_FACTOR = 0.5

// ─── Soroban configuration resolved from env ────────────────────────────────

export interface SorobanConfig {
  contractId: string
  networkPassphrase: string
  sourceAccount: string
  rpcUrl: string
  secretKey: string
  submitPollIntervalMs: number
  submitPollMaxAttempts: number
  rpcTimeoutMs: number
  submitRetry: RetryConfig
}

const positiveIntFromEnv = (key: string, fallback: number): number => {
  const raw = process.env[key]
  if (raw === undefined || raw === '') return fallback

  const parsed = Number.parseInt(raw, 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback
}

const getSubmitRetryConfig = (): RetryConfig => ({
  maxAttempts: positiveIntFromEnv('RETRY_MAX_ATTEMPTS', DEFAULT_SUBMIT_RETRY_MAX_ATTEMPTS),
  initialBackoffMs: positiveIntFromEnv('RETRY_BACKOFF_MS', DEFAULT_SUBMIT_RETRY_BACKOFF_MS),
  maxBackoffMs: positiveIntFromEnv('SOROBAN_SUBMIT_RETRY_MAX_BACKOFF_MS', DEFAULT_SUBMIT_RETRY_MAX_BACKOFF_MS),
  backoffMultiplier: DEFAULT_SUBMIT_RETRY_BACKOFF_MULTIPLIER,
  jitterFactor: DEFAULT_SUBMIT_RETRY_JITTER_FACTOR,
})

/**
 * Returns the Soroban config only when ALL required env vars are present.
 * Acts as the feature-flag: if any var is missing, submit mode is unavailable.
 */
export const getSorobanConfig = (): SorobanConfig | null => {
  const contractId = process.env.SOROBAN_CONTRACT_ID
  const networkPassphrase = process.env.SOROBAN_NETWORK_PASSPHRASE
  const sourceAccount = process.env.SOROBAN_SOURCE_ACCOUNT
  const rpcUrl = process.env.SOROBAN_RPC_URL
  const secretKey = process.env.SOROBAN_SECRET_KEY

  if (!contractId || !networkPassphrase || !sourceAccount || !rpcUrl || !secretKey) {
    return null
  }

  return {
    contractId,
    networkPassphrase,
    sourceAccount,
    rpcUrl,
    secretKey,
    submitPollIntervalMs: positiveIntFromEnv('SOROBAN_SUBMIT_POLL_INTERVAL_MS', DEFAULT_SUBMIT_POLL_INTERVAL_MS),
    submitPollMaxAttempts: positiveIntFromEnv('SOROBAN_SUBMIT_POLL_MAX_ATTEMPTS', DEFAULT_SUBMIT_POLL_MAX_ATTEMPTS),
    rpcTimeoutMs: positiveIntFromEnv('SOROBAN_RPC_TIMEOUT_MS', DEFAULT_RPC_TIMEOUT_MS),
    submitRetry: getSubmitRetryConfig(),
  }
}

/**
 * Whether the backend is configured to submit Soroban transactions.
 * Useful for health checks and observability.
 */
export const isSorobanSubmitEnabled = (): boolean => getSorobanConfig() !== null

// ─── Soroban SDK abstraction (mockable for tests) ───────────────────────────

/**
 * Thin wrapper around the Stellar SDK operations needed for submit.
 * Extracted as a named export so tests can replace it without touching env.
 */
export interface SorobanClient {
  submitVaultCreation(
    config: SorobanConfig,
    args: Record<string, unknown>,
  ): Promise<{ txHash: string }>
}

type StellarSdkLoader = () => Promise<any>

const withRpcTimeout = async <T>(
  operation: Promise<T>,
  operationName: string,
  timeoutMs: number,
): Promise<T> => {
  let timeout: NodeJS.Timeout | undefined

  const timeoutPromise = new Promise<never>((_resolve, reject) => {
    timeout = setTimeout(() => {
      reject(new Error(`Soroban RPC ${operationName} timed out after ${timeoutMs}ms`))
    }, timeoutMs)
  })

  try {
    return await Promise.race([operation, timeoutPromise])
  } finally {
    if (timeout) clearTimeout(timeout)
  }
}

const isRetryableSorobanRpcError = (error: Error): boolean => {
  const message = error.message.toLowerCase()
  return (
    message.includes('timeout') ||
    message.includes('timed out') ||
    message.includes('connection') ||
    message.includes('network') ||
    message.includes('econnreset') ||
    message.includes('econnrefused') ||
    message.includes('enotfound') ||
    message.includes('etimedout') ||
    message.includes('socket') ||
    message.includes('429') ||
    message.includes('rate limit') ||
    message.includes('too many requests') ||
    message.includes('503') ||
    message.includes('502') ||
    message.includes('504')
  )
}

const retryRpc = async <T>(
  operationName: string,
  config: SorobanConfig,
  operation: () => Promise<T>,
): Promise<T> => {
  return retryWithBackoff(
    () => withRpcTimeout(operation(), operationName, config.rpcTimeoutMs),
    config.submitRetry,
    isRetryableSorobanRpcError,
  )
}

/**
 * Default production client that calls the real Stellar SDK.
 * Imported lazily so the module loads even when @stellar/stellar-sdk
 * is not fully configured (e.g. in unit test environments).
 */
export const createDefaultSorobanClient = (
  loadSdk: StellarSdkLoader = () => import('@stellar/stellar-sdk'),
): SorobanClient => ({
  async submitVaultCreation(config, args) {
    // Dynamic import keeps the top-level module lightweight and avoids
    // breaking test suites that never exercise real submission.
    const {
      Keypair,
      Contract,
      rpc: SorobanRpc,
      Networks,
      TransactionBuilder,
      nativeToScVal,
      BASE_FEE,
    } = await loadSdk()

    const server = new SorobanRpc.Server(config.rpcUrl)
    const keypair = Keypair.fromSecret(config.secretKey)
    const account = await retryRpc('getAccount', config, () => server.getAccount(config.sourceAccount))

    const contract = new Contract(config.contractId)
    const callOp = contract.call(
      'create_vault',
      nativeToScVal(args.vaultId, { type: 'string' }),
      nativeToScVal(args.amount, { type: 'string' }),
      nativeToScVal(args.verifier, { type: 'string' }),
      nativeToScVal(args.successDestination, { type: 'string' }),
      nativeToScVal(args.failureDestination, { type: 'string' }),
    )

    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: config.networkPassphrase,
    })
      .addOperation(callOp)
      .setTimeout(30)
      .build()

    const prepared = await retryRpc('prepareTransaction', config, () => server.prepareTransaction(tx))
    prepared.sign(keypair)

    const response = await retryRpc('sendTransaction', config, () => server.sendTransaction(prepared))

    if (response.status === 'ERROR') {
      throw new Error(`Soroban sendTransaction failed: ${response.status}`)
    }

    if (!response.hash) {
      throw new Error('Soroban sendTransaction did not return a transaction hash')
    }

    let getResponse = await retryRpc('getTransaction', config, () => server.getTransaction(response.hash))
    let attempts = 1
    while (getResponse.status === 'NOT_FOUND' && attempts < config.submitPollMaxAttempts) {
      await sleep(config.submitPollIntervalMs)
      getResponse = await retryRpc('getTransaction', config, () => server.getTransaction(response.hash))
      attempts++
    }

    if (getResponse.status !== 'SUCCESS') {
      throw new Error(`Soroban transaction did not succeed: ${getResponse.status}`)
    }

    return { txHash: response.hash }
  },
})

export const defaultSorobanClient: SorobanClient = createDefaultSorobanClient()

// Allow overriding the client (for tests)
let _client: SorobanClient = defaultSorobanClient

export const setSorobanClient = (client: SorobanClient): void => {
  _client = client
}

export const resetSorobanClient = (): void => {
  _client = defaultSorobanClient
}

// ─── Structured logging helper (no PII) ─────────────────────────────────────

const log = (level: 'info' | 'warn' | 'error', event: string, data: Record<string, unknown> = {}): void => {
  const entry = {
    level,
    service: 'disciplr-backend',
    component: 'soroban',
    event,
    ts: new Date().toISOString(),
    ...data,
  }
  if (level === 'error') {
    console.error(JSON.stringify(entry))
  } else {
    console.log(JSON.stringify(entry))
  }
}

// ─── Build payload (existing behaviour, unchanged for mode=build) ───────────

const buildPayload = (
  input: CreateVaultInput,
  vault: PersistedVault,
): VaultCreateResponse['onChain']['payload'] => {
  return {
    contractId: input.onChain?.contractId ?? process.env.SOROBAN_CONTRACT_ID ?? DEFAULT_CONTRACT_ID,
    networkPassphrase:
      input.onChain?.networkPassphrase ?? process.env.SOROBAN_NETWORK_PASSPHRASE ?? 'Test SDF Network ; September 2015',
    sourceAccount: input.onChain?.sourceAccount ?? process.env.SOROBAN_SOURCE_ACCOUNT ?? DEFAULT_SOURCE_ACCOUNT,
    method: 'create_vault',
    args: {
      vaultId: vault.id,
      amount: vault.amount,
      verifier: vault.verifier,
      successDestination: vault.successDestination,
      failureDestination: vault.failureDestination,
      milestones: vault.milestones.map((milestone) => ({
        id: milestone.id,
        title: milestone.title,
        amount: milestone.amount,
        dueDate: milestone.dueDate,
      })),
    },
  }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/**
 * Builds the on-chain payload for vault creation.
 * When mode is 'submit' AND the backend is fully configured, performs an
 * actual Soroban transaction submission. The call is idempotent on the
 * client side: repeated calls with the same vault id produce the same
 * payload structure (the contract itself must enforce on-chain idempotency).
 *
 * Feature-flagged: real submission only occurs when SOROBAN_CONTRACT_ID,
 * SOROBAN_NETWORK_PASSPHRASE, SOROBAN_SOURCE_ACCOUNT, SOROBAN_RPC_URL,
 * and SOROBAN_SECRET_KEY are all set in the environment.
 */
export const buildVaultCreationPayload = async (
  input: CreateVaultInput,
  vault: PersistedVault,
): Promise<VaultCreateResponse['onChain']> => {
  const mode = input.onChain?.mode ?? 'build'
  const payload = buildPayload(input, vault)

  // ── build mode: return signing payload for the client ────────────
  if (mode !== 'submit') {
    return {
      mode,
      payload,
      submission: { attempted: false, status: 'not_requested' },
    }
  }

  // ── submit mode: check feature flag ──────────────────────────────
  const config = getSorobanConfig()
  if (!config) {
    log('warn', 'soroban.submit_not_configured', { vaultId: vault.id })
    return {
      mode,
      payload,
      submission: { attempted: true, status: 'not_configured' },
    }
  }

  // ── submit mode: real submission ─────────────────────────────────
  try {
    log('info', 'soroban.submit_start', { vaultId: vault.id })
    const { txHash } = await _client.submitVaultCreation(config, payload.args)
    log('info', 'soroban.submit_success', { vaultId: vault.id, txHash })

    return {
      mode,
      payload,
      submission: { attempted: true, status: 'success', txHash },
    }
  } catch (err) {
    const appError = AppError.fromContractError(err)
    if (appError) {
      log('error', 'soroban.submit_error_contract', { vaultId: vault.id, code: appError.code, message: appError.message, details: appError.details })
      return {
        mode,
        payload,
        submission: { 
          attempted: true, 
          status: 'error', 
          error: { code: appError.code, message: appError.message, details: appError.details } 
        },
      }
    }

    const message = err instanceof Error ? err.message : 'Unknown submission error'
    log('error', 'soroban.submit_error', { vaultId: vault.id, error: message })

    return {
      mode,
      payload,
      submission: { attempted: true, status: 'error', error: message },
    }
  }
}

// ─── Slash-on-miss payload builder ──────────────────────────────────────────

/**
 * Builds the on-chain payload descriptor for the slash_on_miss contract call.
 * Does NOT submit a real Soroban transaction; submission is gated behind
 * environment configuration the same way as buildVaultCreationPayload.
 * Status is always 'not_configured' until a real submit path is wired.
 */
export const buildSlashOnMissPayload = (vaultId: string) => {
  return {
    mode: 'submit' as const,
    payload: {
      contractId: process.env.SOROBAN_CONTRACT_ID ?? DEFAULT_CONTRACT_ID,
      networkPassphrase: process.env.SOROBAN_NETWORK_PASSPHRASE ?? 'Test SDF Network ; September 2015',
      sourceAccount: process.env.SOROBAN_SOURCE_ACCOUNT ?? DEFAULT_SOURCE_ACCOUNT,
      method: 'slash_on_miss',
      args: { vaultId },
    },
    submission: {
      attempted: true,
      status: 'not_configured' as const,
    },
  }
}
