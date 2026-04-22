import { useQuery } from '@tanstack/react-query';
import { Activity, Database, Shield, ScrollText } from 'lucide-react';
import { api, type Stats, type LogEntry } from '@/lib/api';
import { fmtTime, fmtDuration } from '@/lib/utils';

function KpiCard({
  label,
  value,
  sub,
  icon: Icon,
}: {
  label: string;
  value: string | number;
  sub?: string;
  icon: React.ElementType;
}) {
  return (
    <div className="card flex items-start gap-4">
      <div className="p-3 bg-primary/10 rounded-lg">
        <Icon size={20} className="text-primary" />
      </div>
      <div>
        <div className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          {label}
        </div>
        <div className="text-2xl font-bold mt-1">{value}</div>
        {sub && <div className="text-xs text-muted-foreground mt-1">{sub}</div>}
      </div>
    </div>
  );
}

export default function Dashboard() {
  const { data: stats } = useQuery<Stats>({
    queryKey: ['stats'],
    queryFn: api.stats,
    refetchInterval: 5000,
  });

  const { data: logs } = useQuery<LogEntry[]>({
    queryKey: ['logs'],
    queryFn: api.logs,
    refetchInterval: 5000,
  });

  const storeTotal = stats
    ? stats.store.challenges_used + stats.store.tokens_used
    : 0;

  return (
    <div>
      <h2 className="text-lg font-semibold mb-4">实时监控</h2>
      <div className="text-xs text-muted-foreground mb-4">每 5 秒自动刷新</div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        <KpiCard
          label="站点数"
          value={stats?.sites_count ?? '-'}
          icon={Activity}
        />
        <KpiCard
          label="Store 条目"
          value={storeTotal || '-'}
          sub={
            stats
              ? `挑战: ${stats.store.challenges_used} | Token: ${stats.store.tokens_used} | 上限: ${stats.store.max_entries}`
              : undefined
          }
          icon={Database}
        />
        <KpiCard
          label="追踪 IP"
          value={stats?.risk_ips_tracked ?? '-'}
          icon={Shield}
        />
        <KpiCard
          label="日志条目"
          value={stats?.request_log_count ?? '-'}
          icon={ScrollText}
        />
      </div>

      <div className="card">
        <div className="text-sm font-medium text-muted-foreground mb-3">
          最近请求
        </div>
        <table className="table-base">
          <thead>
            <tr>
              <th>时间</th>
              <th>IP</th>
              <th>站点</th>
              <th>状态</th>
              <th>耗时</th>
            </tr>
          </thead>
          <tbody>
            {logs && logs.length > 0 ? (
              logs.slice(0, 15).map((l, i) => (
                <tr key={i}>
                  <td>{fmtTime(l.timestamp)}</td>
                  <td className="font-mono text-xs">{l.ip || '-'}</td>
                  <td>{l.site_key}</td>
                  <td>
                    <span
                      className={l.success ? 'badge badge-success' : 'badge badge-fail'}
                    >
                      {l.success ? '成功' : '失败'}
                    </span>
                  </td>
                  <td>{fmtDuration(l.duration_ms)}</td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={5} className="text-center py-8 text-muted-foreground">
                  暂无数据
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
