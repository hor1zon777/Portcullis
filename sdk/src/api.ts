import type { ChallengeResponse, VerifyResponse, Challenge } from './types';

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  return (await res.json()) as T;
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
