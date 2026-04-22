import { useState } from 'react';
import { Lock } from 'lucide-react';
import { setToken } from '@/lib/api';

export default function Login({ onSuccess }: { onSuccess: () => void }) {
  const [token, setTokenInput] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!token.trim()) return;
    setLoading(true);
    setError('');
    try {
      const res = await fetch('/admin/api/stats', {
        headers: { authorization: `Bearer ${token}` },
      });
      if (res.status === 401) {
        setError('Token 错误');
      } else if (!res.ok) {
        setError(`HTTP ${res.status}`);
      } else {
        setToken(token);
        onSuccess();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <form onSubmit={handleSubmit} className="card w-96">
        <div className="flex items-center gap-2 mb-6">
          <Lock size={20} className="text-primary" />
          <h1 className="text-lg font-semibold">PoW CAPTCHA Admin</h1>
        </div>
        <label className="block text-sm font-medium mb-2">Admin Token</label>
        <input
          type="password"
          autoFocus
          className="input mb-3"
          value={token}
          onChange={(e) => setTokenInput(e.target.value)}
          placeholder="captcha.toml [admin] token"
        />
        {error && (
          <div className="text-sm text-destructive mb-3">{error}</div>
        )}
        <button type="submit" disabled={loading} className="btn btn-primary w-full">
          {loading ? '验证中...' : '登录'}
        </button>
      </form>
    </div>
  );
}
