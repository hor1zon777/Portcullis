import { Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';

export function Spinner({ className }: { className?: string }) {
  return <Loader2 className={cn('animate-spin text-muted-foreground', className)} size={20} />;
}

export function PageLoader() {
  return (
    <div className="flex items-center justify-center min-h-[300px]">
      <Spinner className="h-8 w-8" />
    </div>
  );
}

export function TableSkeleton({ rows = 5, cols = 4 }: { rows?: number; cols?: number }) {
  return (
    <tbody>
      {Array.from({ length: rows }).map((_, i) => (
        <tr key={i}>
          {Array.from({ length: cols }).map((_, j) => (
            <td key={j} className="px-3 py-3">
              <div className="h-4 bg-muted rounded animate-pulse" style={{ width: `${50 + Math.random() * 40}%` }} />
            </td>
          ))}
        </tr>
      ))}
    </tbody>
  );
}
