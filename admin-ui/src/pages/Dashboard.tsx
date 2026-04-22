import { useQuery } from '@tanstack/react-query';
import { Activity, Database, Shield, ScrollText } from 'lucide-react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { api, type Stats, type LogEntry } from '@/lib/api';
import { fmtTime, fmtDuration, fmtNumber } from '@/lib/utils';
import { PageLoader, TableSkeleton } from '@/components/Spinner';
import { useRef } from 'react';

function KpiCard({ label, value, sub, icon: Icon }: { label: string; value: string | number; sub?: string; icon: React.ElementType }) {
  return (
    <div className="card dark:bg-gray-900 flex items-start gap-4">
      <div className="p-3 bg-primary/10 rounded-lg"><Icon size={20} className="text-primary" /></div>
      <div>
        <div className="text-xs font-medium text-muted-foreground uppercase tracking-wide">{label}</div>
        <div className="text-2xl font-bold mt-1">{value}</div>
        {sub && <div className="text-xs text-muted-foreground mt-1">{sub}</div>}
      </div>
    </div>
  );
}

export default function Dashboard() {
  const historyRef = useRef<{ time: string; total: number; success: number }[]>([]);

  const { data: stats, isLoading: statsLoading } = useQuery<Stats>({
    queryKey: ['stats'], queryFn: api.stats, refetchInterval: 5000,
  });
  const { data: logs, isLoading: logsLoading } = useQuery<LogEntry[]>({
    queryKey: ['logs'], queryFn: api.logs, refetchInterval: 5000,
  });

  if (logs && stats) {
    const now = new Date().toLocaleTimeString('zh-CN', { hour12: false });
    const successCount = logs.filter(l => l.success).length;
    const h = historyRef.current;
    if (h.length === 0 || h[h.length - 1].time !== now) {
      h.push({ time: now, total: logs.length, success: successCount });
      if (h.length > 30) h.shift();
    }
  }

  if (statsLoading) return <PageLoader />;

  const storeTotal = stats ? stats.store.challenges_used + stats.store.tokens_used : 0;
  const successRate = logs && logs.length > 0
    ? ((logs.filter(l => l.success).length / logs.length) * 100).toFixed(1) + '%'
    : '-';

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">实时监控</h2>
      <div className="text-xs text-muted-foreground mb-4">每 5 秒自动刷新</div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <KpiCard label="站点数" value={stats ? fmtNumber(stats.sites_count) : '-'} icon={Activity} />
        <KpiCard label="Store 条目" value={stats ? fmtNumber(storeTotal) : '-'} sub={stats ? `挑战: ${stats.store.challenges_used} | Token: ${stats.store.tokens_used} | 上限: ${fmtNumber(stats.store.max_entries)}` : undefined} icon={Database} />
        <KpiCard label="追踪 IP" value={stats ? fmtNumber(stats.risk_ips_tracked) : '-'} icon={Shield} />
        <KpiCard label="成功率" value={successRate} icon={ScrollText} />
      </div>

      {historyRef.current.length > 1 && (
        <div className="card dark:bg-gray-900 mb-6">
          <div className="text-sm font-medium text-muted-foreground mb-3">请求趋势</div>
          <ResponsiveContainer width="100%" height={200}>
            <LineChart data={historyRef.current}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--border, #e5e7eb)" />
              <XAxis dataKey="time" tick={{ fontSize: 11 }} />
              <YAxis tick={{ fontSize: 11 }} />
              <Tooltip />
              <Line type="monotone" dataKey="total" stroke="hsl(221.2 83.2% 53.3%)" name="总数" strokeWidth={2} dot={false} />
              <Line type="monotone" dataKey="success" stroke="hsl(142 71% 45%)" name="成功" strokeWidth={2} dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}

      <div className="card dark:bg-gray-900">
        <div className="text-sm font-medium text-muted-foreground mb-3">最近请求</div>
        <div className="overflow-x-auto">
          <table className="table-base">
            <thead><tr><th>时间</th><th>IP</th><th>站点</th><th>状态</th><th>耗时</th></tr></thead>
            {logsLoading ? <TableSkeleton cols={5} /> : (
              <tbody>
                {logs && logs.length > 0 ? logs.slice(0, 15).map((l, i) => (
                  <tr key={`${l.timestamp}-${l.nonce}-${i}`}>
                    <td className="whitespace-nowrap">{fmtTime(l.timestamp)}</td>
                    <td className="font-mono text-xs">{l.ip || '-'}</td>
                    <td>{l.site_key}</td>
                    <td><span className={l.success ? 'badge badge-success' : 'badge badge-fail'}>{l.success ? '成功' : '失败'}</span></td>
                    <td>{fmtDuration(l.duration_ms)}</td>
                  </tr>
                )) : <tr><td colSpan={5} className="text-center py-8 text-muted-foreground">暂无数据</td></tr>}
              </tbody>
            )}
          </table>
        </div>
      </div>
    </div>
  );
}
