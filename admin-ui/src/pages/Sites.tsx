import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Trash2 } from 'lucide-react';
import { api, type SiteView } from '@/lib/api';

export default function Sites() {
  const qc = useQueryClient();
  const { data: sites } = useQuery<SiteView[]>({
    queryKey: ['sites'],
    queryFn: api.listSites,
    refetchInterval: 10000,
  });

  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState({
    key: '',
    secret_key: '',
    diff: 18,
    origins: '',
  });

  const createMut = useMutation({
    mutationFn: () =>
      api.createSite({
        ...form,
        origins: form.origins
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sites'] });
      setShowForm(false);
      setForm({ key: '', secret_key: '', diff: 18, origins: '' });
    },
  });

  const deleteMut = useMutation({
    mutationFn: (key: string) => api.deleteSite(key),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['sites'] }),
  });

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">站点管理</h2>
        <button className="btn btn-primary" onClick={() => setShowForm(!showForm)}>
          <Plus size={14} /> 新增站点
        </button>
      </div>

      {showForm && (
        <div className="card mb-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <input
              className="input"
              placeholder="Site Key (pk_xxx)"
              value={form.key}
              onChange={(e) => setForm({ ...form, key: e.target.value })}
            />
            <input
              className="input"
              placeholder="Secret Key (>= 16 字符)"
              type="password"
              value={form.secret_key}
              onChange={(e) => setForm({ ...form, secret_key: e.target.value })}
            />
            <input
              className="input"
              type="number"
              min={8}
              max={28}
              placeholder="Diff"
              value={form.diff}
              onChange={(e) => setForm({ ...form, diff: Number(e.target.value) })}
            />
            <input
              className="input"
              placeholder="Origins (逗号分隔)"
              value={form.origins}
              onChange={(e) => setForm({ ...form, origins: e.target.value })}
            />
          </div>
          <div className="mt-3 flex gap-2">
            <button
              className="btn btn-primary"
              disabled={createMut.isPending}
              onClick={() => createMut.mutate()}
            >
              {createMut.isPending ? '创建中...' : '确认创建'}
            </button>
            <button className="btn btn-secondary" onClick={() => setShowForm(false)}>
              取消
            </button>
          </div>
          {createMut.isError && (
            <div className="text-sm text-destructive mt-2">
              {(createMut.error as Error).message}
            </div>
          )}
        </div>
      )}

      <div className="card p-0">
        <table className="table-base">
          <thead>
            <tr>
              <th>Key</th>
              <th>Diff</th>
              <th>Origins</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {sites && sites.length > 0 ? (
              sites.map((s) => (
                <tr key={s.key}>
                  <td className="font-mono text-xs font-medium">{s.key}</td>
                  <td>{s.diff}</td>
                  <td className="text-xs text-muted-foreground">
                    {s.origins.join(', ') || '(全部)'}
                  </td>
                  <td>
                    <button
                      className="btn btn-destructive btn-sm"
                      onClick={() => {
                        if (confirm(`确认删除站点 ${s.key}？`))
                          deleteMut.mutate(s.key);
                      }}
                    >
                      <Trash2 size={12} /> 删除
                    </button>
                  </td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={4} className="text-center py-8 text-muted-foreground">
                  暂无站点
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
