import { useState } from 'react';
import { Lock, KeyRound } from 'lucide-react';
import {
  setToken,
  setAdminPath,
  getAdminPath,
  probeAuth,
  isValidAdminPath,
} from '@/lib/api';
import { Spinner } from '@/components/Spinner';

export default function Login({ onSuccess }: { onSuccess: () => void }) {
  const [token, setTokenInput] = useState('');
  // 上次登录过会缓存在 localStorage，方便用户少敲一次（仅是 UI 默认值，可改）
  const [adminPath, setAdminPathInput] = useState(getAdminPath() ?? '');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const t = token.trim();
    const p = adminPath.trim();
    if (!t) {
      setError('请输入 Admin Token');
      return;
    }
    if (!isValidAdminPath(p)) {
      setError('Admin 访问路径需为 8-32 位 URL-safe 字符（字母 / 数字 / _ / -）');
      return;
    }
    setLoading(true);
    setError('');
    const ok = await probeAuth(t, p);
    setLoading(false);
    if (ok) {
      setToken(t);
      setAdminPath(p);
      setTokenInput('');
      onSuccess();
    } else {
      setError('Token 或访问路径错误，或服务不可达');
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 via-blue-50/30 to-gray-100 dark:from-gray-950 dark:via-gray-900 dark:to-gray-950 p-4">
      <form onSubmit={handleSubmit} className="card w-full max-w-sm dark:bg-gray-900 dialog-enter shadow-xl">
        <div className="flex items-center gap-2 mb-6">
          <div className="p-2 bg-primary/10 rounded-lg"><Lock size={20} className="text-primary" /></div>
          <h1 className="text-lg font-semibold">PoW CAPTCHA Admin</h1>
        </div>

        <label htmlFor="admin-path" className="block text-sm font-medium mb-2 flex items-center gap-1">
          <KeyRound size={14} className="text-muted-foreground" />
          Admin 访问路径
        </label>
        <input
          id="admin-path"
          type="text"
          autoComplete="off"
          spellCheck={false}
          className="input mb-1 font-mono"
          value={adminPath}
          onChange={(e) => setAdminPathInput(e.target.value)}
          placeholder="服务端日志或 captcha.toml 提供的随机后缀"
          aria-describedby="admin-path-hint"
        />
        <div id="admin-path-hint" className="text-[11px] text-muted-foreground mb-3">
          首次启动时由服务端随机生成（查日志），登录后可在「安全」页面 rotate 或自定义。
        </div>

        <label htmlFor="admin-token" className="block text-sm font-medium mb-2">Admin Token</label>
        <input
          id="admin-token"
          type="password"
          autoFocus
          className="input mb-3"
          value={token}
          onChange={(e) => setTokenInput(e.target.value)}
          placeholder="captcha.toml [admin] token"
          aria-invalid={!!error}
          aria-describedby={error ? 'login-error' : undefined}
        />
        {error && <div id="login-error" className="text-sm text-destructive mb-3" role="alert">{error}</div>}
        <button type="submit" disabled={loading} className="btn btn-primary w-full">
          {loading ? <><Spinner className="h-4 w-4" /> 验证中...</> : '登录'}
        </button>
      </form>
    </div>
  );
}
