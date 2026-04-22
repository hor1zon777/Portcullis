import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Trash2, Pencil, Copy, Check, Eye, EyeOff } from 'lucide-react';
import { toast } from 'sonner';
import { api, type SiteView } from '@/lib/api';
import { copyToClipboard } from '@/lib/utils';
import { PageLoader } from '@/components/Spinner';
import { ConfirmDialog } from '@/components/ConfirmDialog';

function SecretCell({ value }: { value: string }) {
  const [visible, setVisible] = useState(false);
  return (
    <span className="inline-flex items-center gap-1 font-mono text-xs">
      <span className="select-all">{visible ? value : '••••••••••••••••'}</span>
      <button className="text-muted-foreground hover:text-primary" onClick={() => setVisible(!visible)} title={visible ? '隐藏' : '显示'}>
        {visible ? <EyeOff size={12} /> : <Eye size={12} />}
      </button>
      <button className="text-muted-foreground hover:text-primary" onClick={() => { copyToClipboard(value); toast.success('已复制 Secret Key'); }} title="复制">
        <Copy size={12} />
      </button>
    </span>
  );
}

export default function Sites() {
  const qc = useQueryClient();
  const { data: sites, isLoading } = useQuery<SiteView[]>({ queryKey: ['sites'], queryFn: api.listSites, refetchInterval: 10000 });
  const [showForm, setShowForm] = useState(false);
  const [editKey, setEditKey] = useState<string | null>(null);
  const [editDiff, setEditDiff] = useState(18);
  const [editOrigins, setEditOrigins] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [form, setForm] = useState({ diff: 18, origins: '' });

  const createMut = useMutation({
    mutationFn: () => api.createSite({ diff: form.diff, origins: form.origins.split(',').map(s => s.trim()).filter(Boolean) }),
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['sites'] });
      setShowForm(false);
      setForm({ diff: 18, origins: '' });
      toast.success(`站点 ${data.key} 创建成功`, { duration: 5000 });
    },
    onError: (e) => toast.error('创建失败: ' + (e as Error).message),
  });
  const updateMut = useMutation({
    mutationFn: (key: string) => api.updateSite(key, { diff: editDiff, origins: editOrigins.split(',').map(s => s.trim()).filter(Boolean) }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['sites'] }); setEditKey(null); toast.success('站点已更新'); },
    onError: (e) => toast.error('更新失败: ' + (e as Error).message),
  });
  const deleteMut = useMutation({
    mutationFn: (key: string) => api.deleteSite(key),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['sites'] }); setDeleteTarget(null); toast.success('站点已删除'); },
    onError: (e) => toast.error('删除失败: ' + (e as Error).message),
  });

  function startEdit(s: SiteView) { setEditKey(s.key); setEditDiff(s.diff); setEditOrigins(s.origins.join(', ')); }
  function handleCopy(key: string) { copyToClipboard(key); setCopiedKey(key); setTimeout(() => setCopiedKey(null), 2000); toast.success('已复制 ' + key); }
  const formValid = form.diff >= 8 && form.diff <= 28;

  if (isLoading) return <PageLoader />;

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">站点管理</h2>
        <button className="btn btn-primary" onClick={() => setShowForm(!showForm)}><Plus size={14} /> {showForm ? '收起' : '新增站点'}</button>
      </div>
      {showForm && (
        <form className="card dark:bg-gray-900 mb-4" onSubmit={e => { e.preventDefault(); if (formValid) createMut.mutate(); }}>
          <p className="text-sm text-muted-foreground mb-3">Site Key 和 Secret Key 由系统自动生成。</p>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <input className="input" type="number" min={8} max={28} value={form.diff} onChange={e => setForm({ ...form, diff: Number(e.target.value) || 18 })} placeholder="Diff（难度）" />
            <input className="input" placeholder="Origins (逗号分隔，留空放通全部)" value={form.origins} onChange={e => setForm({ ...form, origins: e.target.value })} />
          </div>
          <div className="mt-3 flex gap-2">
            <button type="submit" className="btn btn-primary" disabled={!formValid || createMut.isPending}>{createMut.isPending ? '创建中...' : '确认创建'}</button>
            <button type="button" className="btn btn-secondary" onClick={() => setShowForm(false)}>取消</button>
          </div>
        </form>
      )}
      <div className="card dark:bg-gray-900 p-0">
        <div className="overflow-x-auto">
          <table className="table-base">
            <thead><tr><th>Site Key</th><th>Secret Key</th><th>Diff</th><th>Origins</th><th>操作</th></tr></thead>
            <tbody>
              {sites && sites.length > 0 ? sites.map(s => (
                <tr key={s.key}>
                  <td className="font-mono text-xs font-medium">
                    {s.key}
                    <button className="ml-1 text-muted-foreground hover:text-primary" onClick={() => handleCopy(s.key)}>
                      {copiedKey === s.key ? <Check size={12} className="text-success inline" /> : <Copy size={12} className="inline" />}
                    </button>
                  </td>
                  <td><SecretCell value={s.secret_key} /></td>
                  <td>{editKey === s.key ? <input className="input w-20" type="number" min={8} max={28} value={editDiff} onChange={e => setEditDiff(Number(e.target.value) || 18)} /> : s.diff}</td>
                  <td className="text-xs text-muted-foreground max-w-[200px] truncate" title={s.origins.join(', ')}>{editKey === s.key ? <input className="input" value={editOrigins} onChange={e => setEditOrigins(e.target.value)} /> : s.origins.join(', ') || '(全部)'}</td>
                  <td className="whitespace-nowrap">
                    {editKey === s.key ? (<><button className="btn btn-primary btn-sm mr-1" onClick={() => updateMut.mutate(s.key)} disabled={updateMut.isPending}>保存</button><button className="btn btn-secondary btn-sm" onClick={() => setEditKey(null)}>取消</button></>) : (<><button className="btn btn-secondary btn-sm mr-1" onClick={() => startEdit(s)}><Pencil size={12} /> 编辑</button><button className="btn btn-destructive btn-sm" onClick={() => setDeleteTarget(s.key)}><Trash2 size={12} /> 删除</button></>)}
                  </td>
                </tr>
              )) : <tr><td colSpan={5} className="text-center py-8 text-muted-foreground">暂无站点</td></tr>}
            </tbody>
          </table>
        </div>
      </div>
      <ConfirmDialog open={!!deleteTarget} title={`删除站点 ${deleteTarget}`} description="删除后该站点的前端 widget 将无法获取挑战。" confirmLabel="确认删除" danger onConfirm={async () => { if (deleteTarget) await deleteMut.mutateAsync(deleteTarget); }} onCancel={() => setDeleteTarget(null)} />
    </div>
  );
}
