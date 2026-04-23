import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { KeyRound, Copy, Check, AlertCircle, Sparkles, RefreshCw, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { api, type ManifestPubkey } from '@/lib/api';
import { PageLoader } from '@/components/Spinner';
import { ConfirmDialog } from '@/components/ConfirmDialog';

type Confirm =
  | { kind: 'regenerate' }
  | { kind: 'revoke' }
  | null;

export default function Security() {
  const qc = useQueryClient();
  const { data, isLoading, error } = useQuery<ManifestPubkey>({
    queryKey: ['manifest-pubkey'],
    queryFn: api.manifestPubkey,
  });
  const [copied, setCopied] = useState(false);
  const [confirm, setConfirm] = useState<Confirm>(null);

  const generateMut = useMutation({
    mutationFn: api.generateManifestKey,
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ['manifest-pubkey'] });
      toast.success(res.first_time ? '已生成签名密钥' : '已重新生成签名密钥');
    },
    onError: (e) => toast.error('生成失败：' + (e as Error).message),
  });

  const revokeMut = useMutation({
    mutationFn: api.revokeManifestKey,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['manifest-pubkey'] });
      toast.success('已停用签名密钥');
    },
    onError: (e) => toast.error('停用失败：' + (e as Error).message),
  });

  if (isLoading) return <PageLoader />;
  if (error) {
    return (
      <div className="card dark:bg-gray-900 text-red-500">
        加载失败：{(error as Error).message}
      </div>
    );
  }

  const pubkey = data?.pubkey;

  async function handleCopy() {
    if (!pubkey) return;
    try {
      await navigator.clipboard.writeText(pubkey);
      setCopied(true);
      toast.success('已复制公钥到剪贴板');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error('复制失败，请手动选择复制');
    }
  }

  function handleConfirm() {
    if (!confirm) return;
    const p =
      confirm.kind === 'regenerate'
        ? generateMut.mutateAsync()
        : revokeMut.mutateAsync();
    p.finally(() => setConfirm(null));
  }

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">安全</h2>
      <div className="text-xs text-muted-foreground mb-4">SDK manifest 签名配置</div>

      <div className="card dark:bg-gray-900 slide-up">
        <div className="flex items-start gap-4">
          <div className="p-3 bg-primary/10 rounded-xl">
            <KeyRound size={20} className="text-primary" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 mb-1 flex-wrap">
              <div className="text-sm font-semibold">Manifest 签名公钥</div>
              {data?.enabled ? (
                <span className="badge badge-success">已启用</span>
              ) : (
                <span className="badge badge-fail">未配置</span>
              )}
              <span className="text-[10px] uppercase tracking-wide text-muted-foreground ml-auto">
                {data?.algorithm}
              </span>
            </div>
            <div className="text-xs text-muted-foreground mb-3 leading-relaxed">
              主站接入 Portcullis SDK 时，用此公钥验证{' '}
              <code className="text-[10px] px-1 py-0.5 rounded bg-gray-100 dark:bg-gray-800">
                /sdk/manifest.json
              </code>{' '}
              响应的{' '}
              <code className="text-[10px] px-1 py-0.5 rounded bg-gray-100 dark:bg-gray-800">
                X-Portcullis-Signature
              </code>
              。公钥应通过<strong>带外渠道</strong>复制到主站配置（构建期环境变量、私有仓库里的配置文件等），不要让主站从同一个公开端点读取公钥——那条路径本身可能被篡改。
            </div>

            {pubkey ? (
              <>
                <div className="flex items-center gap-2 flex-wrap">
                  <code className="flex-1 min-w-0 break-all px-3 py-2 rounded-lg bg-gray-100 dark:bg-gray-800 font-mono text-xs">
                    {pubkey}
                  </code>
                  <button
                    onClick={handleCopy}
                    className="shrink-0 inline-flex items-center gap-1.5 px-3 py-2 rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity text-xs font-medium"
                  >
                    {copied ? (
                      <>
                        <Check size={14} /> 已复制
                      </>
                    ) : (
                      <>
                        <Copy size={14} /> 复制
                      </>
                    )}
                  </button>
                </div>
                <div className="mt-2 text-[11px] text-muted-foreground">
                  base64 编码的 32 字节 Ed25519 公钥
                </div>

                <div className="mt-4 pt-4 border-t border-border/50 dark:border-gray-800/50 flex flex-wrap items-center gap-2">
                  <button
                    onClick={() => setConfirm({ kind: 'regenerate' })}
                    disabled={generateMut.isPending || revokeMut.isPending}
                    className="inline-flex items-center gap-1.5 px-3 py-2 rounded-lg bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors text-xs font-medium disabled:opacity-50"
                  >
                    <RefreshCw size={14} /> 重新生成
                  </button>
                  <button
                    onClick={() => setConfirm({ kind: 'revoke' })}
                    disabled={generateMut.isPending || revokeMut.isPending}
                    className="inline-flex items-center gap-1.5 px-3 py-2 rounded-lg text-destructive hover:bg-destructive/10 transition-colors text-xs font-medium disabled:opacity-50"
                  >
                    <Trash2 size={14} /> 停用
                  </button>
                  <div className="text-[11px] text-muted-foreground ml-auto">
                    重新生成或停用后，主站必须同步更新配置，否则 SDK 加载会失败
                  </div>
                </div>
              </>
            ) : (
              <div className="space-y-3">
                <button
                  onClick={() => generateMut.mutate()}
                  disabled={generateMut.isPending}
                  className="inline-flex items-center gap-2 px-4 py-2.5 rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity text-sm font-semibold shadow-md shadow-primary/20 disabled:opacity-60 disabled:cursor-not-allowed"
                >
                  <Sparkles size={16} />
                  {generateMut.isPending ? '生成中…' : '一键生成密钥对'}
                </button>
                <div className="flex items-start gap-2 p-3 rounded-lg bg-blue-50 dark:bg-blue-950/30 text-blue-800 dark:text-blue-300 text-xs">
                  <AlertCircle size={14} className="shrink-0 mt-0.5" />
                  <div>
                    <div className="font-medium mb-1">点击后会发生什么</div>
                    <ul className="list-disc list-inside space-y-0.5 ml-1">
                      <li>服务端生成新的 Ed25519 密钥对</li>
                      <li>私钥 seed 写入 SQLite（重启保留，无需改环境变量）</li>
                      <li>
                        <code className="px-1 py-0.5 rounded bg-blue-100 dark:bg-blue-900/50 font-mono">
                          /sdk/manifest.json
                        </code>{' '}
                        立即开始携带{' '}
                        <code className="px-1 py-0.5 rounded bg-blue-100 dark:bg-blue-900/50 font-mono">
                          X-Portcullis-Signature
                        </code>
                      </li>
                      <li>本页面显示生成后的公钥，复制给主站管理员</li>
                    </ul>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      <ConfirmDialog
        open={confirm?.kind === 'regenerate'}
        title="重新生成签名密钥？"
        description="旧公钥立即失效。已经配置旧公钥的主站会开始拒绝 manifest（SDK 加载失败），直到管理员把新公钥同步过去。建议只在怀疑私钥泄露或主动轮换时执行。"
        confirmLabel="确认重新生成"
        danger
        onConfirm={handleConfirm}
        onCancel={() => setConfirm(null)}
      />
      <ConfirmDialog
        open={confirm?.kind === 'revoke'}
        title="停用签名密钥？"
        description="停用后 manifest 不再携带 X-Portcullis-Signature。已经配置验签公钥并且不做降级处理的主站会直接拒绝加载。只在你确定要关闭签名功能时执行。"
        confirmLabel="确认停用"
        danger
        onConfirm={handleConfirm}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}
