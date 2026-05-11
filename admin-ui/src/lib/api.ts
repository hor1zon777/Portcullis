const TOKEN_KEY = 'captcha_admin_token';
const ADMIN_PATH_KEY = 'captcha_admin_path';
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

/** v1.6.0：admin 后台访问路径后缀。服务端 URL 为 /admin/<suffix>/api/...，
 *  错误的 suffix 直接 404，前端必须先拿到这一段才能调任何 admin API。 */
export function getAdminPath(): string | null {
  return localStorage.getItem(ADMIN_PATH_KEY);
}

export function setAdminPath(suffix: string): void {
  localStorage.setItem(ADMIN_PATH_KEY, suffix);
}

export function clearAdminPath(): void {
  localStorage.removeItem(ADMIN_PATH_KEY);
}

/** 401/404 时触发：让 App 监听并重置认证状态 */
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

/** URL-safe + 8~32 字符的客户端校验，避免明显错误的 suffix 触发 404。
 *  注意服务端会做权威校验，这里只是早期反馈。 */
export function isValidAdminPath(s: string): boolean {
  return /^[A-Za-z0-9_-]{8,32}$/.test(s);
}

/** 构造 /admin/<suffix>/api<path>；若本地没存 suffix 直接抛错让上层走登录页。 */
function buildAdminUrl(path: string): string {
  const suffix = getAdminPath();
  if (!suffix) {
    throw new ApiError(0, '未配置 Admin 访问路径，请重新登录');
  }
  return `/admin/${encodeURIComponent(suffix)}/api${path}`;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getToken();
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);

  try {
    const url = buildAdminUrl(path);
    const res = await fetch(url, {
      ...init,
      signal: ctrl.signal,
      headers: {
        ...(token ? { authorization: `Bearer ${token}` } : {}),
        'content-type': 'application/json',
        ...init.headers,
      },
    });

    // 401 = token 失效；404 = admin path suffix 不对（或已被 rotate）。
    // 两种情况都让用户回登录页重输 token + path。
    if (res.status === 401 || res.status === 404) {
      const reason =
        res.status === 401 ? 'Token 已失效' : 'Admin 访问路径不正确或已变更';
      clearToken();
      clearAdminPath();
      emitAuthChange();
      throw new ApiError(res.status, reason);
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

/** 用于 Login：直接探测一个低成本端点验证 token + admin path 后缀 */
export async function probeAuth(token: string, adminPath: string): Promise<boolean> {
  if (!isValidAdminPath(adminPath)) return false;
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(
      `/admin/${encodeURIComponent(adminPath)}/api/stats`,
      {
        signal: ctrl.signal,
        headers: { authorization: `Bearer ${token}` },
      }
    );
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
  argon2_m_cost: number;
  argon2_t_cost: number;
  argon2_p_cost: number;
  bind_token_to_ip: boolean;
  bind_token_to_ua: boolean;
  /** v1.5.0：secret_key 已 HMAC 化存储（只在创建时返回明文） */
  secret_key_hashed: boolean;
}

export interface AuditEntry {
  id: number;
  ts: number;
  token_prefix: string | null;
  action: string;
  target: string | null;
  ip: string | null;
  success: boolean;
  meta_json: string | null;
}

export interface AuditList {
  total: number;
  entries: AuditEntry[];
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

export interface GenerateManifestKeyResult {
  enabled: boolean;
  pubkey: string;
  algorithm: string;
  /** true = 此前未配置；false = 覆盖了已有密钥 */
  first_time: boolean;
}

export interface AdminPathInfo {
  /** 当前 admin path suffix */
  suffix: string;
  /** 允许的最小 / 最大长度 */
  min_len: number;
  max_len: number;
  /** 字符集说明，仅做提示 */
  charset: string;
}

export interface AdminPathUpdateResult {
  ok: boolean;
  suffix: string;
  note?: string;
}

// ──────── API 调用 ────────

export const api = {
  stats: () => request<Stats>('/stats'),
  listSites: () => request<SiteView[]>('/sites'),
  createSite: (body: {
    diff: number;
    origins: string[];
    argon2_m_cost?: number;
    argon2_t_cost?: number;
    argon2_p_cost?: number;
    bind_token_to_ip?: boolean;
    bind_token_to_ua?: boolean;
  }) => request<{ ok: boolean; key: string; secret_key: string }>('/sites', { method: 'POST', body: JSON.stringify(body) }),
  updateSite: (key: string, body: { diff?: number; origins?: string[]; argon2_m_cost?: number; argon2_t_cost?: number; argon2_p_cost?: number; bind_token_to_ip?: boolean; bind_token_to_ua?: boolean }) =>
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
  generateManifestKey: () =>
    request<GenerateManifestKeyResult>('/manifest-pubkey/generate', { method: 'POST' }),
  revokeManifestKey: () =>
    request<{ ok: boolean; removed: boolean }>('/manifest-pubkey', { method: 'DELETE' }),
  audit: (params?: { limit?: number; offset?: number; action?: string }) => {
    const qs = new URLSearchParams();
    if (params?.limit !== undefined) qs.set('limit', String(params.limit));
    if (params?.offset !== undefined) qs.set('offset', String(params.offset));
    if (params?.action) qs.set('action', params.action);
    const suffix = qs.toString() ? `?${qs.toString()}` : '';
    return request<AuditList>(`/audit${suffix}`);
  },
  adminPathGet: () => request<AdminPathInfo>('/admin-path'),
  adminPathUpdate: (suffix: string) =>
    request<AdminPathUpdateResult>('/admin-path', {
      method: 'PUT',
      body: JSON.stringify({ suffix }),
    }),
  adminPathRotate: () =>
    request<AdminPathUpdateResult>('/admin-path/rotate', { method: 'POST' }),
};
