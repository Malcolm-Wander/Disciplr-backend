import { NotificationProvider } from './provider.js';
import { retryWithBackoff, DEFAULT_RETRY_CONFIG } from '../../utils/retry.js';

/**
 * EmailNotificationProvider implements the NotificationProvider interface.
 * It sends email notifications. Currently this is a stub that simulates latency.
 * Transient SMTP 4xx errors are retried using exponential backoff with jitter.
 * 5xx errors are considered permanent and are not retried, preserving dead‑letter semantics.
 */
export class EmailNotificationProvider implements NotificationProvider {
  readonly name = 'email';

  /**
   * Send an email notification.
   * @param recipient - Email address of the recipient.
   * @param subject   - Subject line.
   * @param body      - Email body.
   */
  async send(recipient: string, subject: string, body: string): Promise<void> {
    // Wrap the actual send operation with retry logic.
    await retryWithBackoff(
      () => this.performSend(recipient, subject, body),
      DEFAULT_RETRY_CONFIG,
      (err) => {
        const code = (err as any).statusCode;
        return typeof code === 'number' && code >= 400 && code < 500; // retryable 4xx
      }
    );
  }

  /**
   * Stub implementation of the low‑level SMTP send.
   * In a real implementation this would invoke nodemailer or another SMTP client.
   * It may throw an error with a `statusCode` property to indicate SMTP response.
   */
  private async performSend(recipient: string, subject: string, body: string): Promise<void> {
    // Simulate network latency.
    await new Promise((resolve) => setTimeout(resolve, 50));
    // Stubbed success – in real code this could throw errors.
    console.log(`[EmailProvider] Sent to ${recipient}: ${subject}`);
  }
}
