import { useState, useRef, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Trash2, Pencil, Copy, Check, Eye, EyeOff, Info } from 'lucide-react';
import { toast } from 'sonner';
import { api, type SiteView } from '@/lib/api';
import { copyToClipboard } from '@/lib/utils';
import { PageLoader } from '@/components/Spinner';
import { ConfirmDialog } from '@/components/ConfirmDialog';

function SecretCell({ value, hashed }: { value: string; hashed?: boolean }) {
  const [visible, setVisible] = useState(false);
  if (hashed) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
        <span className="font-mono italic">(已哈希存储)</span>
        <span title="此站点是早期 v1.5.0 期间创建，secret_key 已 HMAC 化无法恢复明文；如需查看请删除后重建。新建站点会保留明文以便随时查看。" className="cursor-help">ⓘ</span>
      </span>
    );
  }
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

function Tooltip({ text }: { text: string }) {
  const triggerRef = useRef<HTMLSpanElement>(null);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const rect = triggerRef.current.getBoundingClientRect();
    setPos({ top: rect.top, left: rect.left + rect.width / 2 });
  }, [open]);

  return (
    <>
      <span
        ref={triggerRef}
        className="inline-flex ml-1 cursor-help align-middle"
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        tabIndex={0}
      >
        <Info size={12} className="text-muted-foreground" />
      </span>
      {open && pos && createPortal(
        <span
          role="tooltip"
          style={{
            position: 'fixed',
            top: pos.top - 6,
            left: pos.left,
            transform: 'translate(-50%, -100%)',
          }}
          className="z-[9999] w-56 rounded bg-gray-900 text-white text-xs p-2 shadow-lg pointer-events-none whitespace-normal"
        >
          {text}
        </span>,
        document.body
      )}
    </>
  );
}

const ARGON2_HINTS = {
  m_cost: 'Argon2id 内存成本 (KiB)。范围 8-65536。值越大越安全但客户端求解越慢。参考：4096≈5ms, 19456≈20ms, 65536≈80ms',
  t_cost: 'Argon2id 迭代次数。范围 1-10。增加迭代会线性增加求解耗时',
  p_cost: 'Argon2id 并行度。当前固定为 1（串行模式）',
};

const BIND_HINTS = {
  ip: '开启后 /verify 签发的 token 会绑定发放时的 client IP（sha256 前 16 字节）；/siteverify 必须携带 client_ip 且一致。须确保反向代理正确透传 X-Forwarded-For / X-Real-IP。',
  ua: '开启后 token 绑定发放时的 User-Agent（sha256 前 8 字节）。UA 稳定性弱（浏览器自动升级会变），评估后再开。',
};

export default function Sites() {
  const qc = useQueryClient();
  const { data: sites, isLoading } = useQuery<SiteView[]>({ queryKey: ['sites'], queryFn: api.listSites, refetchInterval: 10000 });
  const [showForm, setShowForm] = useState(false);
  const [editKey, setEditKey] = useState<string | null>(null);
  const [editDiff, setEditDiff] = useState(18);
  const [editOrigins, setEditOrigins] = useState('');
  const [editMCost, setEditMCost] = useState(19456);
  const [editTCost, setEditTCost] = useState(2);
  const [editBindIp, setEditBindIp] = useState(false);
  const [editBindUa, setEditBindUa] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [form, setForm] = useState({ diff: 18, origins: '', m_cost: 19456, t_cost: 2, bind_ip: false, bind_ua: false });

  const createMut = useMutation({
    mutationFn: () => api.createSite({
      diff: form.diff,
      origins: form.origins.split(',').map(s => s.trim()).filter(Boolean),
      argon2_m_cost: form.m_cost,
      argon2_t_cost: form.t_cost,
      argon2_p_cost: 1,
      bind_token_to_ip: form.bind_ip,
      bind_token_to_ua: form.bind_ua,
    }),
    onSuccess: (data) => {
      qc.invalidateQueries({ queryKey: ['sites'] });
      setShowForm(false);
      setForm({ diff: 18, origins: '', m_cost: 19456, t_cost: 2, bind_ip: false, bind_ua: false });
      // 创建即复制一份到剪贴板方便立即接入；列表行也可随时点眼睛重新查看。
      copyToClipboard(data.secret_key);
      toast.success(
        `站点 ${data.key} 已创建，Secret Key 已复制到剪贴板。`,
        { duration: 8000 }
      );
    },
    onError: (e) => toast.error('创建失败: ' + (e as Error).message),
  });
  const updateMut = useMutation({
    mutationFn: (key: string) => api.updateSite(key, {
      diff: editDiff,
      origins: editOrigins.split(',').map(s => s.trim()).filter(Boolean),
      argon2_m_cost: editMCost,
      argon2_t_cost: editTCost,
      argon2_p_cost: 1,
      bind_token_to_ip: editBindIp,
      bind_token_to_ua: editBindUa,
    }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['sites'] }); setEditKey(null); toast.success('站点已更新'); },
    onError: (e) => toast.error('更新失败: ' + (e as Error).message),
  });
  const deleteMut = useMutation({
    mutationFn: (key: string) => api.deleteSite(key),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['sites'] }); setDeleteTarget(null); toast.success('站点已删除'); },
    onError: (e) => toast.error('删除失败: ' + (e as Error).message),
  });

  function startEdit(s: SiteView) {
    setEditKey(s.key);
    setEditDiff(s.diff);
    setEditOrigins(s.origins.join(', '));
    setEditMCost(s.argon2_m_cost);
    setEditTCost(s.argon2_t_cost);
    setEditBindIp(s.bind_token_to_ip);
    setEditBindUa(s.bind_token_to_ua);
  }
  function handleCopy(key: string) { copyToClipboard(key); setCopiedKey(key); setTimeout(() => setCopiedKey(null), 2000); toast.success('已复制 ' + key); }
  const formValid = form.diff >= 8 && form.diff <= 28 && form.m_cost >= 8 && form.m_cost <= 65536 && form.t_cost >= 1 && form.t_cost <= 10;

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
            <div>
              <label className="text-xs font-medium text-muted-foreground">Diff（难度）</label>
              <input className="input" type="number" min={8} max={28} value={form.diff} onChange={e => setForm({ ...form, diff: Number(e.target.value) || 18 })} />
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground">Origins (逗号分隔)</label>
              <input className="input" placeholder="留空放通全部" value={form.origins} onChange={e => setForm({ ...form, origins: e.target.value })} />
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground inline-flex items-center">m_cost (KiB)<Tooltip text={ARGON2_HINTS.m_cost} /></label>
              <input className="input" type="number" min={8} max={65536} value={form.m_cost} onChange={e => setForm({ ...form, m_cost: Number(e.target.value) || 19456 })} />
            </div>
            <div>
              <label className="text-xs font-medium text-muted-foreground inline-flex items-center">t_cost (迭代)<Tooltip text={ARGON2_HINTS.t_cost} /></label>
              <input className="input" type="number" min={1} max={10} value={form.t_cost} onChange={e => setForm({ ...form, t_cost: Number(e.target.value) || 2 })} />
            </div>
            <div className="md:col-span-2 flex flex-wrap items-center gap-4 pt-1">
              <label className="inline-flex items-center gap-2 text-xs font-medium text-muted-foreground cursor-pointer">
                <input type="checkbox" checked={form.bind_ip} onChange={e => setForm({ ...form, bind_ip: e.target.checked })} />
                绑定 client IP<Tooltip text={BIND_HINTS.ip} />
              </label>
              <label className="inline-flex items-center gap-2 text-xs font-medium text-muted-foreground cursor-pointer">
                <input type="checkbox" checked={form.bind_ua} onChange={e => setForm({ ...form, bind_ua: e.target.checked })} />
                绑定 User-Agent<Tooltip text={BIND_HINTS.ua} />
              </label>
            </div>
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
            <thead>
              <tr>
                <th>Site Key</th>
                <th>Secret Key</th>
                <th>Diff</th>
                <th className="whitespace-nowrap">m_cost<Tooltip text={ARGON2_HINTS.m_cost} /></th>
                <th className="whitespace-nowrap">t_cost<Tooltip text={ARGON2_HINTS.t_cost} /></th>
                <th className="whitespace-nowrap">绑定<Tooltip text="token 绑定 client IP / UA（opt-in）" /></th>
                <th>Origins</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {sites && sites.length > 0 ? sites.map(s => (
                <tr key={s.key}>
                  <td className="font-mono text-xs font-medium">
                    {s.key}
                    <button className="ml-1 text-muted-foreground hover:text-primary" onClick={() => handleCopy(s.key)}>
                      {copiedKey === s.key ? <Check size={12} className="text-success inline" /> : <Copy size={12} className="inline" />}
                    </button>
                  </td>
                  <td><SecretCell value={s.secret_key} hashed={s.secret_key_hashed} /></td>
                  <td>{editKey === s.key ? <input className="input w-20" type="number" min={8} max={28} value={editDiff} onChange={e => setEditDiff(Number(e.target.value) || 18)} /> : s.diff}</td>
                  <td>{editKey === s.key ? <input className="input w-24" type="number" min={8} max={65536} value={editMCost} onChange={e => setEditMCost(Number(e.target.value) || 19456)} /> : <span className="font-mono text-xs">{s.argon2_m_cost}</span>}</td>
                  <td>{editKey === s.key ? <input className="input w-16" type="number" min={1} max={10} value={editTCost} onChange={e => setEditTCost(Number(e.target.value) || 2)} /> : <span className="font-mono text-xs">{s.argon2_t_cost}</span>}</td>
                  <td className="whitespace-nowrap text-xs">
                    {editKey === s.key ? (
                      <span className="inline-flex flex-col gap-1">
                        <label className="inline-flex items-center gap-1 cursor-pointer"><input type="checkbox" checked={editBindIp} onChange={e => setEditBindIp(e.target.checked)} />IP</label>
                        <label className="inline-flex items-center gap-1 cursor-pointer"><input type="checkbox" checked={editBindUa} onChange={e => setEditBindUa(e.target.checked)} />UA</label>
                      </span>
                    ) : (
                      <span className="font-mono">
                        {s.bind_token_to_ip ? 'IP' : '—'}
                        {' / '}
                        {s.bind_token_to_ua ? 'UA' : '—'}
                      </span>
                    )}
                  </td>
                  <td className="text-xs text-muted-foreground max-w-[200px] truncate" title={s.origins.join(', ')}>{editKey === s.key ? <input className="input" value={editOrigins} onChange={e => setEditOrigins(e.target.value)} /> : s.origins.join(', ') || '(全部)'}</td>
                  <td className="whitespace-nowrap">
                    {editKey === s.key ? (<><button className="btn btn-primary btn-sm mr-1" onClick={() => updateMut.mutate(s.key)} disabled={updateMut.isPending}>保存</button><button className="btn btn-secondary btn-sm" onClick={() => setEditKey(null)}>取消</button></>) : (<><button className="btn btn-secondary btn-sm mr-1" onClick={() => startEdit(s)}><Pencil size={12} /> 编辑</button><button className="btn btn-destructive btn-sm" onClick={() => setDeleteTarget(s.key)}><Trash2 size={12} /> 删除</button></>)}
                  </td>
                </tr>
              )) : <tr><td colSpan={8} className="text-center py-8 text-muted-foreground">暂无站点</td></tr>}
            </tbody>
          </table>
        </div>
      </div>
      <ConfirmDialog open={!!deleteTarget} title={`删除站点 ${deleteTarget}`} description="删除后该站点的前端 widget 将无法获取挑战。" confirmLabel="确认删除" danger onConfirm={async () => { if (deleteTarget) await deleteMut.mutateAsync(deleteTarget); }} onCancel={() => setDeleteTarget(null)} />
    </div>
  );
}
