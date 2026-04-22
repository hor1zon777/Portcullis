import type { ChallengeResponse, VerifyResponse, Challenge } from './types';

const DEFAULT_TIMEOUT_MS = 10_000;
const DEFAULT_RETRIES = 1;

export class HttpError extends Error {
  constructor(
    public status: number,
    public body: string,
    public url: string
  ) {
    super(`HTTP ${status} on ${url}: ${body.slice(0, 200)}`);
    this.name = 'HttpError';
  }
}

/** 5xx 视为可重试，4xx 不重试（业务/配置错误） */
function isRetryable(err: unknown): boolean {
  if (err instanceof HttpError) return err.status >= 500;
  if (err instanceof DOMException && err.name === 'AbortError') return true;
  // TypeError 通常是网络错误
  return err instanceof TypeError;
}

async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs: number
): Promise<Response> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    return await fetch(url, { ...init, signal: ctrl.signal });
  } finally {
    clearTimeout(timer);
  }
}

async function postJson<T>(
  url: string,
  body: unknown,
  opts: { timeoutMs?: number; retries?: number } = {}
): Promise<T> {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const retries = opts.retries ?? DEFAULT_RETRIES;
  let lastErr: unknown;

  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      const res = await fetchWithTimeout(
        url,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(body),
        },
        timeoutMs
      );
      if (!res.ok) {
        const text = await res.text();
        throw new HttpError(res.status, text, url);
      }
      return (await res.json()) as T;
    } catch (err) {
      lastErr = err;
      if (attempt < retries && isRetryable(err)) {
        // 指数退避（200ms, 400ms...）
        await new Promise((r) => setTimeout(r, 200 * Math.pow(2, attempt)));
        continue;
      }
      throw err;
    }
  }
  throw lastErr;
}

export async function fetchChallenge(
  endpoint: string,
  siteKey: string
): Promise<ChallengeResponse> {
  return postJson<ChallengeResponse>(`${endpoint}/api/v1/challenge`, {
    site_key: siteKey,
  });
}

export async function submitVerify(
  endpoint: string,
  challenge: Challenge,
  sig: string,
  nonce: number
): Promise<VerifyResponse> {
  return postJson<VerifyResponse>(`${endpoint}/api/v1/verify`, {
    challenge,
    sig,
    nonce,
  });
}
