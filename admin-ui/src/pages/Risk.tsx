import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { ShieldBan, ShieldCheck, Ban, Undo2 } from 'lucide-react';
import { toast } from 'sonner';
import { api, type RiskData } from '@/lib/api';
import { isValidIpOrCidr } from '@/lib/utils';
import { PageLoader } from '@/components/Spinner';
import { ConfirmDialog } from '@/components/ConfirmDialog';

export default function Risk() {
  const qc = useQueryClient();
  const [blockInput, setBlockInput] = useState('');
  const [confirmTarget, setConfirmTarget] = useState<{ ip: string; action: 'block' | 'unblock' } | null>(null);
  const { data, isLoading } = useQuery<RiskData>({ queryKey: ['risk'], queryFn: api.riskIps, refetchInterval: 5000 });

  const blockMut = useMutation({
    mutationFn: (ip: string) => api.blockIp(ip),
    onSuccess: (_, ip) => { qc.invalidateQueries({ queryKey: ['risk'] }); setBlockInput(''); toast.success(`已封禁 ${ip}`); },
    onError: (e) => toast.error('封禁失败: ' + (e as Error).message),
  });
  const unblockMut = useMutation({
    mutationFn: (ip: string) => api.unblockIp(ip),
    onSuccess: (_, ip) => { qc.invalidateQueries({ queryKey: ['risk'] }); toast.success(`已解封 ${ip}`); },
    onError: (e) => toast.error('解封失败: ' + (e as Error).message),
  });

  function handleConfirm() {
    if (!confirmTarget) return;
    const p = confirmTarget.action === 'block' ? blockMut.mutateAsync(confirmTarget.ip) : unblockMut.mutateAsync(confirmTarget.ip);
    p.finally(() => setConfirmTarget(null));
  }

  const inputValid = blockInput.trim() && isValidIpOrCidr(blockInput);
  if (isLoading) return <PageLoader />;

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">IP 风控</h2>
      <div className="card dark:bg-gray-900 mb-4">
        <div className="text-sm font-medium mb-2">封禁 IP</div>
        <div className="flex gap-2">
          <input className="input max-w-sm" placeholder="IP 或 CIDR（如 1.2.3.4 或 10.0.0.0/8）" value={blockInput} onChange={e => setBlockInput(e.target.value)} onKeyDown={e => { if (e.key === 'Enter' && inputValid) setConfirmTarget({ ip: blockInput.trim(), action: 'block' }); }} />
          <button className="btn btn-destructive" disabled={!inputValid || blockMut.isPending} onClick={() => setConfirmTarget({ ip: blockInput.trim(), action: 'block' })}><Ban size={14} /> 封禁</button>
        </div>
        {blockInput && !inputValid && <p className="text-xs text-destructive mt-1">请输入有效的 IP 或 CIDR</p>}
      </div>

      <div className="card dark:bg-gray-900 p-0 mb-4">
        <div className="px-4 py-3 border-b border-border dark:border-gray-800"><span className="text-sm font-medium">被追踪的 IP</span><span className="text-xs text-muted-foreground ml-2">{data?.ips.length ?? 0} 个</span></div>
        <div className="overflow-x-auto">
          <table className="table-base">
            <thead><tr><th>IP</th><th>总次数</th><th>失败</th><th>失败率</th><th>Extra Diff</th><th>状态</th><th>操作</th></tr></thead>
            <tbody>
              {data?.ips && data.ips.length > 0 ? data.ips.map(ip => (
                <tr key={ip.ip}>
                  <td className="font-mono text-xs">{ip.ip}</td>
                  <td>{ip.total}</td>
                  <td>{ip.fails}</td>
                  <td>{(ip.fail_rate * 100).toFixed(1)}%</td>
                  <td>{ip.extra_diff > 0 ? <span className="badge badge-fail">+{ip.extra_diff}</span> : '-'}</td>
                  <td>{ip.is_blocked ? <span className="badge badge-fail"><ShieldBan size={12} className="mr-1" />封禁</span> : ip.is_allowed ? <span className="badge badge-success"><ShieldCheck size={12} className="mr-1" />白名单</span> : <span className="badge badge-muted">正常</span>}</td>
                  <td>{ip.is_blocked ? <button className="btn btn-secondary btn-sm" onClick={() => setConfirmTarget({ ip: ip.ip, action: 'unblock' })}><Undo2 size={12} /> 解封</button> : <button className="btn btn-destructive btn-sm" onClick={() => setConfirmTarget({ ip: ip.ip, action: 'block' })}><Ban size={12} /> 封禁</button>}</td>
                </tr>
              )) : <tr><td colSpan={7} className="text-center py-8 text-muted-foreground">暂无追踪 IP</td></tr>}
            </tbody>
          </table>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="card dark:bg-gray-900">
          <div className="text-sm font-medium mb-2">黑名单</div>
          {data?.blocked && data.blocked.length > 0 ? <div className="flex flex-wrap gap-2">{data.blocked.map(b => <span key={b} className="inline-flex items-center gap-1 px-2 py-1 bg-red-50 dark:bg-red-950 text-red-700 dark:text-red-300 rounded text-xs font-mono">{b} <button className="hover:text-red-900" onClick={() => setConfirmTarget({ ip: b, action: 'unblock' })}>&times;</button></span>)}</div> : <div className="text-sm text-muted-foreground">无</div>}
        </div>
        <div className="card dark:bg-gray-900">
          <div className="text-sm font-medium mb-2">白名单</div>
          {data?.allowed && data.allowed.length > 0 ? <div className="flex flex-wrap gap-2">{data.allowed.map(a => <span key={a} className="px-2 py-1 bg-green-50 dark:bg-green-950 text-green-700 dark:text-green-300 rounded text-xs font-mono">{a}</span>)}</div> : <div className="text-sm text-muted-foreground">无</div>}
        </div>
      </div>

      <ConfirmDialog open={!!confirmTarget} title={confirmTarget?.action === 'block' ? `封禁 ${confirmTarget?.ip}` : `解封 ${confirmTarget?.ip}`} description={confirmTarget?.action === 'block' ? '封禁后该 IP 的所有请求将被拒绝。' : '解封后该 IP 可以正常使用。'} confirmLabel={confirmTarget?.action === 'block' ? '确认封禁' : '确认解封'} danger={confirmTarget?.action === 'block'} onConfirm={handleConfirm} onCancel={() => setConfirmTarget(null)} />
    </div>
  );
}
