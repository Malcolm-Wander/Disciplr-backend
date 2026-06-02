import { z } from "zod";

/**
 * Coerces a string env var to a positive integer, returning the default
 * if the raw value is missing or not a valid positive number.
 */
const positiveInt = (fallback: number) =>
  z
    .string()
    .optional()
    .transform((v) => {
      if (v === undefined || v === "") return fallback;
      const n = Number.parseInt(v, 10);
      return Number.isFinite(n) && n > 0 ? n : fallback;
    });

/** Coerces a string env var to a non-negative integer. */
const nonNegativeInt = (fallback: number) =>
  z
    .string()
    .optional()
    .transform((v) => {
      if (v === undefined || v === "") return fallback;
      const n = Number.parseInt(v, 10);
      return Number.isFinite(n) && n >= 0 ? n : fallback;
    });

/** Validates that a string is a valid http:// or https:// URL. */
const httpUrl = () =>
  z
    .string()
    .refine(
      (url) => /^https?:\/\/./.test(url),
      'must be a valid HTTP or HTTPS URL (e.g., https://example.com)',
    );

/** Schema for all environment variables consumed by the application. */
export const envSchema = z
  .object({
    // ── Core ────────────────────────────────────────────────────
    NODE_ENV: z
      .enum(["development", "production", "test"])
      .default("development"),
    PORT: positiveInt(3000),
    SERVICE_NAME: z.string().default("disciplr-backend"),
    DATABASE_URL: z.string().min(1, "DATABASE_URL is required").refine(
      (url) => url.startsWith('postgres://') || url.startsWith('postgresql://'),
      'DATABASE_URL must be a valid PostgreSQL connection URL'
    ),

    // ── Auth / secrets ──────────────────────────────────────
    JWT_SECRET: z.string().min(16, "must be at least 16 characters").default("change-me-in-production-long-secret"),
    JWT_ACCESS_SECRET: z.string().min(16, "must be at least 16 characters").default("fallback-access-secret"),
    JWT_REFRESH_SECRET: z.string().min(16, "must be at least 16 characters").default("fallback-refresh-secret"),
    JWT_ACCESS_EXPIRES_IN: z.string().regex(/^\d+[smhd]$/, "invalid duration format").default("15m"),
    JWT_REFRESH_EXPIRES_IN: z.string().regex(/^\d+[smhd]$/, "invalid duration format").default("7d"),
    DOWNLOAD_SECRET: z.string().min(16, "must be at least 16 characters").default("change-me-in-production-long-secret"),

    // JWT key rotation support – JSON encoded array of {kid, secret, retiredAt?}
    JWT_KEYS: z
      .string()
      .optional()
      .transform((val) => {
        if (!val) return [];
        try {
          const parsed = JSON.parse(val);
          if (!Array.isArray(parsed)) throw new Error("JWT_KEYS must be an array");
          return parsed.map((item: any) => {
            const ret = item.retiredAt ? new Date(item.retiredAt) : undefined;
            return { kid: item.kid, secret: item.secret, retiredAt: ret };
          });
        } catch (e) {
          throw new Error(`Invalid JWT_KEYS JSON: ${(e as Error).message}`);
        }
      }),

    // ── Horizon / Stellar ─────────────────────────────────────
    HORIZON_URL: z.string().optional().refine(
      (url) => !url || url.startsWith('http://') || url.startsWith('https://'),
      'HORIZON_URL must be a valid HTTP or HTTPS URL'
    ),
    CORS_ORIGINS: z.string().optional().refine(
      (val) => {
        if (val === undefined) return true;
        if (val === "") return false;
        if (val === '*') return true;
        const parts = val.split(',');
        return parts.length > 0 && parts.every(p => p.trim().startsWith('http'));
      },
      'CORS_ORIGINS cannot be empty'
    ),
    CONTRACT_ADDRESS: z.string().optional(),
    START_LEDGER: nonNegativeInt(0).optional(),
    RETRY_MAX_ATTEMPTS: nonNegativeInt(3),
    RETRY_BACKOFF_MS: nonNegativeInt(100),

    // ── Soroban ────────────────────────────────────────────────
    SOROBAN_CONTRACT_ID: z.string().optional().refine(
      (v) => !v || /^C[0-9A-Z]{55}$/.test(v),
      'must be a valid Soroban contract ID (56-char base32 starting with C)'
    ),
    SOROBAN_NETWORK_PASSPHRASE: z.string().optional(),
    SOROBAN_SOURCE_ACCOUNT: z.string().optional(),
    SOROBAN_RPC_URL: httpUrl().optional(),
    SOROBAN_SECRET_KEY: z.string().optional(),
    SOROBAN_SUBMIT_POLL_INTERVAL_MS: positiveInt(1_000),
    SOROBAN_SUBMIT_POLL_MAX_ATTEMPTS: positiveInt(30),
    SOROBAN_RPC_TIMEOUT_MS: positiveInt(30_000),
    SOROBAN_SUBMIT_RETRY_MAX_BACKOFF_MS: positiveInt(5_000),
    STELLAR_NETWORK_PASSPHRASE: z.string().optional(),

    // ── Job system ───────────────────────────────────────────────
    JOB_WORKER_CONCURRENCY: positiveInt(2),
    JOB_QUEUE_POLL_INTERVAL_MS: positiveInt(250),
    JOB_HISTORY_LIMIT: positiveInt(50),
    ENABLE_JOB_SCHEDULER: z.string().optional(),

    // ── ETL ───────────────────────────────────────────────────────
    ETL_INTERVAL_MINUTES: positiveInt(5),
    ENABLE_ETL_WORKER: z.string().optional(),
    ETL_BACKFILL_FROM: z.string().optional(),
    ETL_BACKFILL_TO: z.string().optional(),

    // ── Security thresholds ───────────────────────────────────
    SECURITY_RATE_LIMIT_WINDOW_MS: positiveInt(60_000),
    SECURITY_RATE_LIMIT_MAX_REQUESTS: positiveInt(120),
    SECURITY_SUSPICIOUS_WINDOW_MS: positiveInt(300_000),
    SECURITY_SUSPICIOUS_404_THRESHOLD: positiveInt(20),
    SECURITY_SUSPICIOUS_DISTINCT_PATH_THRESHOLD: positiveInt(12),
    SECURITY_SUSPICIOUS_BAD_REQUEST_THRESHOLD: positiveInt(30),
    SECURITY_SUSPICIOUS_HIGH_VOLUME_THRESHOLD: positiveInt(300),
    SECURITY_FAILED_LOGIN_WINDOW_MS: positiveInt(900_000),
    SECURITY_FAILED_LOGIN_BURST_THRESHOLD: positiveInt(5),
    SECURITY_ALERT_COOLDOWN_MS: positiveInt(300_000),
    ORG_RATE_LIMIT_MAX: positiveInt(200),
    ORG_RATE_LIMIT_WINDOW_MS: positiveInt(60000),
    EXPORT_DAILY_QUOTA_LIMIT: positiveInt(100),

    // ── Deadline / Analytics schedulers ───────────────────────
    DEADLINE_CHECK_INTERVAL_MS: positiveInt(60_000),
    ANALYTICS_RECOMPUTE_INTERVAL_MS: positiveInt(300_000),

    // ── Misc / Limits ───────────────────────────────────────
    MAX_JSON_BODY_SIZE: z.string().default('500kb'),
    HORIZON_LAG_THRESHOLD: nonNegativeInt(10),
    HORIZON_SHUTDOWN_TIMEOUT_MS: positiveInt(30_000),
  })
  .superRefine((data, ctx) => {
    // Existing CORS warning
    if (data.NODE_ENV === "production" && data.CORS_ORIGINS === "*") {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["CORS_ORIGINS"],
        message: 'CORS_ORIGINS cannot be "*" in production environment',
      });
    }
    // Additional validation for JWT_KEYS could be added here if needed
  });

export type Env = z.infer<typeof envSchema>;
export type JwtKey = { kid: string; secret: string; retiredAt?: Date };

/** Returns parsed JWT keys from the environment. */
export function getJwtKeys(env: Env): JwtKey[] {
  // The envSchema already transformed JWT_KEYS into an array of objects.
  // TypeScript cannot infer that, so we cast.
  return (env as any).JWT_KEYS as JwtKey[];
}
