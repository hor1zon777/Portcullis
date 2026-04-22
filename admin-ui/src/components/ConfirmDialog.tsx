import { useState, useCallback, type ReactNode } from 'react';
import { AlertTriangle } from 'lucide-react';

interface Props {
  open: boolean;
  title: string;
  description?: string;
  confirmLabel?: string;
  danger?: boolean;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
  children?: ReactNode;
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = '确认',
  danger = false,
  onConfirm,
  onCancel,
}: Props) {
  const [loading, setLoading] = useState(false);

  const handleConfirm = useCallback(async () => {
    setLoading(true);
    try {
      await onConfirm();
    } finally {
      setLoading(false);
    }
  }, [onConfirm]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onCancel}>
      <div className="card max-w-sm w-full mx-4" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-start gap-3 mb-4">
          {danger && <AlertTriangle size={20} className="text-destructive mt-0.5 flex-shrink-0" />}
          <div>
            <h3 className="font-semibold">{title}</h3>
            {description && (
              <p className="text-sm text-muted-foreground mt-1">{description}</p>
            )}
          </div>
        </div>
        <div className="flex justify-end gap-2">
          <button className="btn btn-secondary" onClick={onCancel} disabled={loading}>
            取消
          </button>
          <button
            className={`btn ${danger ? 'btn-destructive' : 'btn-primary'}`}
            onClick={handleConfirm}
            disabled={loading}
          >
            {loading ? '处理中...' : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
