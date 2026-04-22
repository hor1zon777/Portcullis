import { useState } from 'react';
import { Lock } from 'lucide-react';
import { setToken, probeAuth } from '@/lib/api';
import { Spinner } from '@/components/Spinner';

export default function Login({ onSuccess }: { onSuccess: () => void }) {
  const [input, setInput] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const val = input.trim();
    if (!val) { setError('请输入 Admin Token'); return; }
    setLoading(true);
    setError('');
    const ok = await probeAuth(val);
    setLoading(false);
    if (ok) {
      setToken(val);
      setInput('');
      onSuccess();
    } else {
      setError('Token 错误或服务不可达');
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gradient-to-br from-gray-50 via-blue-50/30 to-gray-100 dark:from-gray-950 dark:via-gray-900 dark:to-gray-950">
      <form onSubmit={handleSubmit} className="card w-96 dark:bg-gray-900 dialog-enter shadow-xl">
        <div className="flex items-center gap-2 mb-6">
          <div className="p-2 bg-primary/10 rounded-lg"><Lock size={20} className="text-primary" /></div>
          <h1 className="text-lg font-semibold">PoW CAPTCHA Admin</h1>
        </div>
        <label htmlFor="admin-token" className="block text-sm font-medium mb-2">Admin Token</label>
        <input
          id="admin-token"
          type="password"
          autoFocus
          className="input mb-3"
          value={input}
          onChange={(e) => setInput(e.target.value)}
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
