import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { api, type LogEntry } from '@/lib/api';
import { fmtTime, fmtDuration } from '@/lib/utils';

export default function Logs() {
  const [filter, setFilter] = useState<'all' | 'success' | 'fail'>('all');

  const { data: logs } = useQuery<LogEntry[]>({
    queryKey: ['logs'],
    queryFn: api.logs,
    refetchInterval: 5000,
  });

  const filtered = logs?.filter((l) => {
    if (filter === 'success') return l.success;
    if (filter === 'fail') return !l.success;
    return true;
  });

  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">请求日志</h2>
        <div className="flex gap-1">
          {(['all', 'success', 'fail'] as const).map((f) => (
            <button
              key={f}
              className={`btn btn-sm ${filter === f ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setFilter(f)}
            >
              {{ all: '全部', success: '成功', fail: '失败' }[f]}
            </button>
          ))}
        </div>
      </div>
      <div className="text-xs text-muted-foreground mb-3">
        每 5 秒自动刷新，最近 200 条
      </div>

      <div className="card p-0">
        <table className="table-base">
          <thead>
            <tr>
              <th>时间</th>
              <th>IP</th>
              <th>站点</th>
              <th>Nonce</th>
              <th>状态</th>
              <th>耗时</th>
            </tr>
          </thead>
          <tbody>
            {filtered && filtered.length > 0 ? (
              filtered.map((l, i) => (
                <tr key={i}>
                  <td className="whitespace-nowrap">{fmtTime(l.timestamp)}</td>
                  <td className="font-mono text-xs">{l.ip || '-'}</td>
                  <td>{l.site_key}</td>
                  <td className="font-mono text-xs">{l.nonce}</td>
                  <td>
                    <span className={l.success ? 'badge badge-success' : 'badge badge-fail'}>
                      {l.success ? '成功' : '失败'}
                    </span>
                  </td>
                  <td>{fmtDuration(l.duration_ms)}</td>
                </tr>
              ))
            ) : (
              <tr>
                <td colSpan={6} className="text-center py-8 text-muted-foreground">
                  暂无日志
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
