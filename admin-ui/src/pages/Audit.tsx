import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { ClipboardList } from 'lucide-react';
import { api, type AuditList, type AuditEntry } from '@/lib/api';
import { PageLoader } from '@/components/Spinner';

const ACTION_OPTIONS: Array<{ value: string; label: string }> = [
  { value: '', label: '全部' },
  { value: 'site.create', label: '站点创建' },
  { value: 'site.update', label: '站点更新' },
  { value: 'site.delete', label: '站点删除' },
  { value: 'ip.block', label: 'IP 封禁' },
  { value: 'ip.unblock', label: 'IP 解封' },
  { value: 'manifest.generate', label: 'Manifest 签发' },
  { value: 'manifest.revoke', label: 'Manifest 撤销' },
  { value: 'login.fail', label: '登录失败' },
];

const PAGE_SIZE = 100;

function ActionBadge({ action, success }: { action: string; success: boolean }) {
  const color = success
    ? action.startsWith('login.fail')
      ? 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/40 dark:text-yellow-300'
      : 'bg-green-100 text-green-800 dark:bg-green-900/40 dark:text-green-300'
    : 'bg-red-100 text-red-800 dark:bg-red-900/40 dark:text-red-300';
  return (
    <span className={`inline-block px-2 py-0.5 rounded font-mono text-xs ${color}`}>
      {action}
    </span>
  );
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleString('zh-CN', { hour12: false });
}

export default function Audit() {
  const [actionFilter, setActionFilter] = useState('');
  const [page, setPage] = useState(0);
  const { data, isLoading } = useQuery<AuditList>({
    queryKey: ['audit', actionFilter, page],
    queryFn: () =>
      api.audit({
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
        action: actionFilter || undefined,
      }),
    refetchInterval: 15000,
  });

  if (isLoading) return <PageLoader />;

  const total = data?.total ?? 0;
  const entries: AuditEntry[] = data?.entries ?? [];
  const maxPage = Math.max(0, Math.ceil(total / PAGE_SIZE) - 1);

  return (
    <div>
      <div className="flex items-center justify-between mb-4 flex-wrap gap-2">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <ClipboardList size={18} /> 管理员审计
          <span className="text-xs font-normal text-muted-foreground">共 {total} 条</span>
        </h2>
        <div className="flex items-center gap-2">
          <label className="text-xs text-muted-foreground">过滤：</label>
          <select
            className="input w-40"
            value={actionFilter}
            onChange={(e) => {
              setActionFilter(e.target.value);
              setPage(0);
            }}
          >
            {ACTION_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>{o.label}</option>
            ))}
          </select>
        </div>
      </div>

      <div className="card dark:bg-gray-900 p-0">
        <div className="overflow-x-auto">
          <table className="table-base">
            <thead>
              <tr>
                <th className="whitespace-nowrap">时间</th>
                <th>操作</th>
                <th>目标</th>
                <th>来源 IP</th>
                <th className="whitespace-nowrap">Token</th>
                <th>附加</th>
              </tr>
            </thead>
            <tbody>
              {entries.length > 0 ? entries.map((e) => (
                <tr key={e.id}>
                  <td className="whitespace-nowrap text-xs font-mono text-muted-foreground">
                    {formatTime(e.ts)}
                  </td>
                  <td><ActionBadge action={e.action} success={e.success} /></td>
                  <td className="font-mono text-xs">{e.target ?? '-'}</td>
                  <td className="font-mono text-xs">{e.ip ?? '-'}</td>
                  <td className="font-mono text-xs text-muted-foreground">{e.token_prefix ?? '-'}</td>
                  <td className="text-xs text-muted-foreground max-w-[260px] truncate" title={e.meta_json ?? ''}>
                    {e.meta_json ?? '-'}
                  </td>
                </tr>
              )) : (
                <tr><td colSpan={6} className="text-center py-8 text-muted-foreground">暂无审计记录</td></tr>
              )}
            </tbody>
          </table>
        </div>
      </div>

      {maxPage > 0 && (
        <div className="flex items-center justify-between mt-3 text-xs">
          <span className="text-muted-foreground">
            第 {page + 1} / {maxPage + 1} 页
          </span>
          <div className="flex gap-2">
            <button className="btn btn-secondary btn-sm" disabled={page === 0} onClick={() => setPage(page - 1)}>上一页</button>
            <button className="btn btn-secondary btn-sm" disabled={page >= maxPage} onClick={() => setPage(page + 1)}>下一页</button>
          </div>
        </div>
      )}
    </div>
  );
}
