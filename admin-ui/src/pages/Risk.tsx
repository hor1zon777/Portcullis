import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { ShieldBan, ShieldCheck, Ban, Undo2 } from 'lucide-react';
import { api, type RiskData } from '@/lib/api';

export default function Risk() {
  const qc = useQueryClient();
  const [blockInput, setBlockInput] = useState('');

  const { data } = useQuery<RiskData>({
    queryKey: ['risk'],
    queryFn: api.riskIps,
    refetchInterval: 5000,
  });

  const blockMut = useMutation({
    mutationFn: (ip: string) => api.blockIp(ip),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['risk'] });
      setBlockInput('');
    },
  });

  const unblockMut = useMutation({
    mutationFn: (ip: string) => api.unblockIp(ip),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['risk'] }),
  });

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">IP 风控</h2>

      <div className="card mb-4">
        <div className="text-sm font-medium mb-2">封禁 IP</div>
        <div className="flex gap-2">
          <input
            className="input max-w-sm"
            placeholder="IP 或 CIDR（如 1.2.3.4 或 10.0.0.0/8）"
            value={blockInput}
            onChange={(e) => setBlockInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && blockInput && blockMut.mutate(blockInput)}
          />
          <button
            className="btn btn-destructive"
            disabled={blockMut.isPending || !blockInput}
            onClick={() => blockMut.mutate(blockInput)}
          >
            <Ban size={14} /> 封禁
          </button>
        </div>
      </div>

      <div className="card p-0 mb-4">
        <div className="px-4 py-3 border-b border-border">
          <span className="text-sm font-medium">被追踪的 IP</span>
          <span className="text-xs text-muted-foreground ml-2">
            {data?.ips.length ?? 0} 个
          </span>
        </div>
        <table className="table-base">
          <thead>
            <tr>
              <th>IP</th>
              <th>总次数</th>
              <th>失败</th>
              <th>失败率</th>
              <th>Extra Diff</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {data?.ips && data.ips.length > 0 ? (
              data.ips.map((ip) => (
                <tr key={ip.ip}>
                  <td className="font-mono text-xs">{ip.ip}</td>
                  <td>{ip.total}</td>
                  <td>{ip.fails}</td>
                  <td>{(ip.fail_rate * 100).toFixed(1)}%</td>
                  <td>
                    {ip.extra_diff > 0 ? (
                      <span className="badge badge-fail">+{ip.extra_diff}</span>
                    ) : (
                      '-'
                    )}
                  </td>
                  <td>
                    {ip.is_blocked ? (
                      <span className="badge badge-fail">
                        <ShieldBan size={12} className="mr-1" /> 封禁
                      </span>
                    ) : ip.is_allowed ? (
                      <span className="badge badge-success">
                        <ShieldCheck size={12} className="mr-1" /> 白名单
                      </span>
                    ) : (
                      <span className="badge badge-muted">正常</span>
                    )}
                  </td>
                  <td>
                    {ip.is_blocked ? (
                      <button
                        className="btn btn-secondary btn-sm"
                        onClick={() => unblockMut.mutate(ip.ip)}
                      >
                        <Undo2 size={12} /> 解封
                      </button>
                    ) : (
                      <button
                        className="btn btn-destructive btn-sm"
                        onClick={() => blockMut.mutate(ip.ip)}
                      >
                        <Ban size={12} /> 封禁
                      </button>
                    )}
                  </td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={7} className="text-center py-8 text-muted-foreground">
                  暂无追踪 IP
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="card">
          <div className="text-sm font-medium mb-2">黑名单</div>
          {data?.blocked && data.blocked.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {data.blocked.map((b) => (
                <span
                  key={b}
                  className="inline-flex items-center gap-1 px-2 py-1 bg-red-50 text-red-700 rounded text-xs font-mono"
                >
                  {b}
                  <button
                    className="hover:text-red-900"
                    onClick={() => unblockMut.mutate(b)}
                  >
                    &times;
                  </button>
                </span>
              ))}
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">无</div>
          )}
        </div>
        <div className="card">
          <div className="text-sm font-medium mb-2">白名单</div>
          {data?.allowed && data.allowed.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {data.allowed.map((a) => (
                <span
                  key={a}
                  className="px-2 py-1 bg-green-50 text-green-700 rounded text-xs font-mono"
                >
                  {a}
                </span>
              ))}
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">无</div>
          )}
        </div>
      </div>
    </div>
  );
}
