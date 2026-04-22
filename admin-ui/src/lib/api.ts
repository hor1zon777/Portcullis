const TOKEN_KEY = 'captcha_admin_token';

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
  }
}

async function request<T>(
  path: string,
  init: RequestInit = {}
): Promise<T> {
  const token = getToken();
  const res = await fetch(`/admin/api${path}`, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...init.headers,
    },
  });
  if (res.status === 401) {
    clearToken();
    window.location.reload();
    throw new ApiError(401, 'Unauthorized');
  }
  if (!res.ok) {
    const text = await res.text();
    throw new ApiError(res.status, text);
  }
  return res.json();
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
  diff: number;
  origins: string[];
  has_secret: boolean;
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

// ──────── API 调用 ────────

export const api = {
  stats: () => request<Stats>('/stats'),
  listSites: () => request<SiteView[]>('/sites'),
  createSite: (body: {
    key: string;
    secret_key: string;
    diff: number;
    origins: string[];
  }) =>
    request<{ ok: boolean }>('/sites', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  updateSite: (key: string, body: { diff?: number; origins?: string[] }) =>
    request<{ ok: boolean }>(`/sites/${encodeURIComponent(key)}`, {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
  deleteSite: (key: string) =>
    request<{ ok: boolean }>(`/sites/${encodeURIComponent(key)}`, {
      method: 'DELETE',
    }),
  logs: () => request<LogEntry[]>('/logs'),
  riskIps: () => request<RiskData>('/risk/ips'),
  blockIp: (ip: string) =>
    request<{ ok: boolean }>('/risk/block', {
      method: 'POST',
      body: JSON.stringify({ ip }),
    }),
  unblockIp: (ip: string) =>
    request<{ ok: boolean }>('/risk/block', {
      method: 'DELETE',
      body: JSON.stringify({ ip }),
    }),
};
