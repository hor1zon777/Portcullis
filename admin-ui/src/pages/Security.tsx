import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { KeyRound, Copy, Check, AlertCircle } from 'lucide-react';
import { toast } from 'sonner';
import { api, type ManifestPubkey } from '@/lib/api';
import { PageLoader } from '@/components/Spinner';

export default function Security() {
  const { data, isLoading, error } = useQuery<ManifestPubkey>({
    queryKey: ['manifest-pubkey'],
    queryFn: api.manifestPubkey,
  });
  const [copied, setCopied] = useState(false);

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
              。公钥应通过<strong>带外渠道</strong>复制到主站配置（构建期环境变量、私有仓库里的配置文件等），
              不要让主站从同一个公开端点读取公钥——那条路径本身可能被篡改。
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
              </>
            ) : (
              <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-50 dark:bg-amber-950/30 text-amber-800 dark:text-amber-300 text-xs">
                <AlertCircle size={14} className="shrink-0 mt-0.5" />
                <div>
                  <div className="font-medium mb-1.5">签名未启用</div>
                  <div className="mb-1">启用步骤：</div>
                  <ol className="list-decimal list-inside space-y-1 ml-1">
                    <li>
                      运行{' '}
                      <code className="px-1 py-0.5 rounded bg-amber-100 dark:bg-amber-900/50 font-mono">
                        captcha-server gen-manifest-key
                      </code>{' '}
                      生成密钥对
                    </li>
                    <li>
                      把输出的私钥 seed 写入环境变量{' '}
                      <code className="px-1 py-0.5 rounded bg-amber-100 dark:bg-amber-900/50 font-mono">
                        CAPTCHA_MANIFEST_SIGNING_KEY
                      </code>{' '}
                      或 captcha.toml 的{' '}
                      <code className="px-1 py-0.5 rounded bg-amber-100 dark:bg-amber-900/50 font-mono">
                        [server].manifest_signing_key
                      </code>
                    </li>
                    <li>重启服务（或等配置热重载生效），本页即会显示公钥</li>
                  </ol>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
