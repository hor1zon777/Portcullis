import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function fmtTime(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const sameDay =
    d.getFullYear() === today.getFullYear() &&
    d.getMonth() === today.getMonth() &&
    d.getDate() === today.getDate();
  if (sameDay) {
    return d.toLocaleTimeString('zh-CN', { hour12: false });
  }
  return d.toLocaleString('zh-CN', { hour12: false });
}

export function fmtRelative(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 60_000) return `${Math.floor(diff / 1000)}秒前`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}小时前`;
  return `${Math.floor(diff / 86_400_000)}天前`;
}

export function fmtDuration(ms: number): string {
  if (ms < 1) return ms.toFixed(2) + 'ms';
  if (ms < 1000) return ms.toFixed(1) + 'ms';
  return (ms / 1000).toFixed(2) + 's';
}

export function fmtNumber(n: number): string {
  return n.toLocaleString('zh-CN');
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

const IP_RE = /^(\d{1,3}\.){3}\d{1,3}(\/\d{1,2})?$|^[0-9a-f:]+(\/\d{1,3})?$/i;
export function isValidIpOrCidr(s: string): boolean {
  return IP_RE.test(s.trim());
}

export function isValidSiteKey(s: string): boolean {
  return /^[a-zA-Z0-9_]{3,64}$/.test(s);
}
