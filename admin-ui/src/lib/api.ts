const TOKEN_KEY = 'captcha_admin_token';
const TIMEOUT_MS = 10_000;

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

/** 401 时触发：让 App 监听并重置认证状态 */
const AUTH_EVENT = 'captcha-admin-auth-changed';
export function onAuthChange(handler: () => void): () => void {
  window.addEventListener(AUTH_EVENT, handler);
  return () => window.removeEventListener(AUTH_EVENT, handler);
}
function emitAuthChange() {
  window.dispatchEvent(new Event(AUTH_EVENT));
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public body?: unknown
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getToken();
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);

  try {
    const res = await fetch(`/admin/api${path}`, {
      ...init,
      signal: ctrl.signal,
      headers: {
        ...(token ? { authorization: `Bearer ${token}` } : {}),
        'content-type': 'application/json',
        ...init.headers,
      },
    });

    if (res.status === 401) {
      clearToken();
      emitAuthChange();
      throw new ApiError(401, '未授权或 Token 已失效');
    }

    if (!res.ok) {
      let body: unknown = undefined;
      let msg = `HTTP ${res.status}`;
      try {
        const text = await res.text();
        try {
          body = JSON.parse(text);
          if (body && typeof body === 'object' && 'error' in body) {
            msg = String((body as { error: unknown }).error);
          }
        } catch {
          msg = text || msg;
        }
      } catch {
        // ignore
      }
      throw new ApiError(res.status, msg, body);
    }

    if (res.status === 204) return undefined as T;
    return (await res.json()) as T;
  } catch (err) {
    if (err instanceof ApiError) throw err;
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw new ApiError(0, '请求超时（10s）');
    }
    throw new ApiError(0, err instanceof Error ? err.message : String(err));
  } finally {
    clearTimeout(timer);
  }
}

/** 用于 Login：直接探测一个低成本端点验证 token */
export async function probeAuth(token: string): Promise<boolean> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
  try {
    const res = await fetch('/admin/api/stats', {
      signal: ctrl.signal,
      headers: { authorization: `Bearer ${token}` },
    });
    return res.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

// ──────── 类型定义 ────────

export interface StoreMetrics {
  challenges_used: number;
  tokens_used: number;
  max_entries: number;
}

export interface Stats {
  store: StoreMetrics;
  risk_ips_tracked: number;
  request_log_count: number;
  sites_count: number;
}

export interface SiteView {
  key: string;
  secret_key: string;
  diff: number;
  origins: string[];
}

export interface LogEntry {
  timestamp: number;
  ip: string | null;
  site_key: string;
  nonce: number;
  success: boolean;
  duration_ms: number;
  error: string | null;
}

export interface IpRiskSummary {
  ip: string;
  total: number;
  fails: number;
  fail_rate: number;
  extra_diff: number;
  is_blocked: boolean;
  is_allowed: boolean;
}

export interface RiskData {
  ips: IpRiskSummary[];
  blocked: string[];
  allowed: string[];
}

export interface ManifestPubkey {
  /** 是否已配置 manifest 签名私钥 */
  enabled: boolean;
  /** base64 编码的 32 字节 Ed25519 公钥；未启用时 undefined */
  pubkey?: string;
  /** 签名算法，固定 "ed25519" */
  algorithm: string;
}

// ──────── API 调用 ────────

export const api = {
  stats: () => request<Stats>('/stats'),
  listSites: () => request<SiteView[]>('/sites'),
  createSite: (body: {
    diff: number;
    origins: string[];
  }) => request<{ ok: boolean; key: string; secret_key: string }>('/sites', { method: 'POST', body: JSON.stringify(body) }),
  updateSite: (key: string, body: { diff?: number; origins?: string[] }) =>
    request<{ ok: boolean }>(`/sites/${encodeURIComponent(key)}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
  deleteSite: (key: string) =>
    request<{ ok: boolean }>(`/sites/${encodeURIComponent(key)}`, { method: 'DELETE' }),
  logs: () => request<LogEntry[]>('/logs'),
  riskIps: () => request<RiskData>('/risk/ips'),
  blockIp: (ip: string) =>
    request<{ ok: boolean }>('/risk/block', { method: 'POST', body: JSON.stringify({ ip }) }),
  unblockIp: (ip: string) =>
    request<{ ok: boolean }>('/risk/block', { method: 'DELETE', body: JSON.stringify({ ip }) }),
  manifestPubkey: () => request<ManifestPubkey>('/manifest-pubkey'),
};
