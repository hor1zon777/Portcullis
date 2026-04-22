import { useState, useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Search } from 'lucide-react';
import { api, type LogEntry } from '@/lib/api';
import { fmtTime, fmtDuration } from '@/lib/utils';
import { PageLoader } from '@/components/Spinner';

export default function Logs() {
  const [statusFilter, setStatusFilter] = useState<'all' | 'success' | 'fail'>('all');
  const [ipSearch, setIpSearch] = useState('');
  const [siteFilter, setSiteFilter] = useState('');

  const { data: logs, isLoading } = useQuery<LogEntry[]>({ queryKey: ['logs'], queryFn: api.logs, refetchInterval: 5000 });

  const siteKeys = useMemo(() => logs ? [...new Set(logs.map(l => l.site_key))] : [], [logs]);

  const filtered = useMemo(() => {
    if (!logs) return [];
    return logs.filter(l => {
      if (statusFilter === 'success' && !l.success) return false;
      if (statusFilter === 'fail' && l.success) return false;
      if (ipSearch && !(l.ip || '').includes(ipSearch)) return false;
      if (siteFilter && l.site_key !== siteFilter) return false;
      return true;
    });
  }, [logs, statusFilter, ipSearch, siteFilter]);

  if (isLoading) return <PageLoader />;

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">请求日志</h2>
      <div className="text-xs text-muted-foreground mb-4">每 5 秒刷新 | 显示 {filtered.length} / {logs?.length ?? 0} 条</div>
      <div className="flex flex-wrap gap-2 mb-4">
        {(['all', 'success', 'fail'] as const).map(f => (
          <button key={f} className={`btn btn-sm ${statusFilter === f ? 'btn-primary' : 'btn-secondary'}`} onClick={() => setStatusFilter(f)}>
            {{ all: '全部', success: '成功', fail: '失败' }[f]}
          </button>
        ))}
        <div className="relative">
          <Search size={14} className="absolute left-2.5 top-2 text-muted-foreground" />
          <input className="input pl-8 w-40" placeholder="搜索 IP" value={ipSearch} onChange={e => setIpSearch(e.target.value)} />
        </div>
        {siteKeys.length > 1 && (
          <select className="input w-auto" value={siteFilter} onChange={e => setSiteFilter(e.target.value)}>
            <option value="">全部站点</option>
            {siteKeys.map(k => <option key={k} value={k}>{k}</option>)}
          </select>
        )}
      </div>
      <div className="card dark:bg-gray-900 p-0">
        <div className="overflow-x-auto">
          <table className="table-base">
            <thead><tr><th>时间</th><th>IP</th><th>站点</th><th>Nonce</th><th>状态</th><th>耗时</th><th>错误</th></tr></thead>
            <tbody>
              {filtered.length > 0 ? filtered.map((l, i) => (
                <tr key={`${l.timestamp}-${l.nonce}-${i}`}>
                  <td className="whitespace-nowrap">{fmtTime(l.timestamp)}</td>
                  <td className="font-mono text-xs">{l.ip || '-'}</td>
                  <td>{l.site_key}</td>
                  <td className="font-mono text-xs">{l.nonce}</td>
                  <td><span className={l.success ? 'badge badge-success' : 'badge badge-fail'}>{l.success ? '成功' : '失败'}</span></td>
                  <td>{fmtDuration(l.duration_ms)}</td>
                  <td className="text-xs text-muted-foreground max-w-[150px] truncate" title={l.error || ''}>{l.error || '-'}</td>
                </tr>
              )) : <tr><td colSpan={7} className="text-center py-8 text-muted-foreground">{logs && logs.length > 0 ? '无匹配结果' : '暂无日志'}</td></tr>}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
