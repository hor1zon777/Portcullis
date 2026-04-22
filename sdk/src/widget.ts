/** Widget 渲染与状态机（chunked 主线程求解，无 Worker 依赖） */

import { fetchChallenge, submitVerify } from './api';
import { t } from './i18n';
import { injectStyles } from './styles';
import type { CaptchaOptions, CaptchaWidget } from './types';

type State = 'idle' | 'running' | 'success' | 'error' | 'unsupported';
const CHUNK_SIZE = 5000;

function expectedAttempts(diff: number): number {
  return Math.pow(2, diff);
}

function wasmSupported(): boolean {
  return (
    typeof WebAssembly === 'object' &&
    typeof WebAssembly.instantiate === 'function'
  );
}

type WasmModule = {
  default: (input?: unknown) => Promise<unknown>;
  init: () => void;
  create_solver: (
    payloadJson: string,
    hardLimit: bigint
  ) => {
    step: (chunkSize: bigint) => {
      found: boolean;
      nonce?: number;
      hash?: string;
      attempts: number;
      elapsed_ms?: number;
      exhausted: boolean;
    };
    challenge_json: () => string;
    sig: () => string;
    free: () => void;
  };
};

/** WASM 实例按 wasmBase 缓存，避免多 endpoint 串实例 */
const wasmCache = new Map<string, Promise<WasmModule>>();

async function loadWasm(wasmBase: string): Promise<WasmModule> {
  const cached = wasmCache.get(wasmBase);
  if (cached) return cached;
  const promise = (async () => {
    const mod: WasmModule = await import(
      /* @vite-ignore */ `${wasmBase}/captcha_wasm.js`
    );
    await mod.default();
    mod.init();
    return mod;
  })();
  wasmCache.set(wasmBase, promise);
  try {
    return await promise;
  } catch (e) {
    // 加载失败时移除 cache，允许下次重试
    wasmCache.delete(wasmBase);
    throw e;
  }
}

export class Widget implements CaptchaWidget {
  private container: HTMLElement;
  private el!: HTMLDivElement;
  private label!: HTMLDivElement;
  private progress!: HTMLDivElement;
  private bar!: HTMLSpanElement;

  private state: State = 'idle';
  private token: string | null = null;
  private destroyed = false;
  private pendingTimers = new Set<ReturnType<typeof setTimeout>>();

  constructor(
    container: HTMLElement,
    private opts: Required<
      Omit<CaptchaOptions, 'onSuccess' | 'onError' | 'onExpired'>
    > & {
      onSuccess?: CaptchaOptions['onSuccess'];
      onError?: CaptchaOptions['onError'];
      onExpired?: CaptchaOptions['onExpired'];
    }
  ) {
    this.container = container;
    injectStyles();
    this.render();
    if (!wasmSupported()) {
      this.setState('unsupported', t(this.opts.lang, 'unsupported'));
    }
  }

  private setTimer(fn: () => void, ms: number): ReturnType<typeof setTimeout> {
    const id = setTimeout(() => {
      this.pendingTimers.delete(id);
      fn();
    }, ms);
    this.pendingTimers.add(id);
    return id;
  }

  private render() {
    const wrap = document.createElement('div');
    wrap.className = 'powc-widget powc-idle';
    wrap.setAttribute('data-theme', this.opts.theme);
    wrap.setAttribute('role', 'button');
    wrap.setAttribute('tabindex', '0');
    wrap.setAttribute('aria-label', t(this.opts.lang, 'aria_widget_label'));
    wrap.setAttribute('aria-pressed', 'false');

    const check = document.createElement('div');
    check.className = 'powc-check';
    check.setAttribute('aria-hidden', 'true');

    const labelWrap = document.createElement('div');
    labelWrap.className = 'powc-label';

    const label = document.createElement('div');
    label.setAttribute('aria-live', 'polite');
    label.setAttribute('aria-atomic', 'true');
    label.textContent = t(this.opts.lang, 'click_to_verify');

    const progress = document.createElement('div');
    progress.className = 'powc-progress';
    progress.style.display = 'none';
    progress.setAttribute('role', 'progressbar');
    progress.setAttribute('aria-valuemin', '0');
    progress.setAttribute('aria-valuemax', '100');
    progress.setAttribute('aria-valuenow', '0');
    const bar = document.createElement('span');
    progress.appendChild(bar);

    const brand = document.createElement('span');
    brand.className = 'powc-brand';
    brand.setAttribute('aria-hidden', 'true');
    brand.textContent = t(this.opts.lang, 'powered_by');

    labelWrap.appendChild(label);
    labelWrap.appendChild(progress);
    wrap.appendChild(check);
    wrap.appendChild(labelWrap);
    wrap.appendChild(brand);

    wrap.addEventListener('click', () => this.start());
    wrap.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        this.start();
      }
    });

    this.container.appendChild(wrap);
    this.el = wrap;
    this.label = label;
    this.progress = progress;
    this.bar = bar;
  }

  private setState(s: State, labelText?: string) {
    this.state = s;
    this.el.classList.remove(
      'powc-idle',
      'powc-running',
      'powc-success',
      'powc-error',
      'powc-unsupported'
    );
    this.el.classList.add(`powc-${s}`);
    if (labelText !== undefined) this.label.textContent = labelText;
    this.progress.style.display = s === 'running' ? 'block' : 'none';
    this.bar.style.width = '0%';
    this.progress.setAttribute('aria-valuenow', '0');
    this.el.setAttribute('aria-busy', s === 'running' ? 'true' : 'false');
    this.el.setAttribute('aria-pressed', s === 'success' ? 'true' : 'false');
    if (s === 'unsupported') {
      this.el.setAttribute('aria-disabled', 'true');
      this.el.removeAttribute('tabindex');
    }
  }

  private updateProgress(pct: number) {
    const v = Math.min(100, pct);
    this.bar.style.width = v.toFixed(1) + '%';
    this.progress.setAttribute('aria-valuenow', Math.round(v).toString());
  }

  private reportError(err: Error) {
    this.setState('error', t(this.opts.lang, 'failed'));
    this.opts.onError?.(err);
  }

  async start(): Promise<void> {
    if (
      this.destroyed ||
      this.state === 'running' ||
      this.state === 'success' ||
      this.state === 'unsupported'
    ) {
      return;
    }
    this.setState('running', t(this.opts.lang, 'verifying'));
    this.token = null;

    try {
      const wasm = await loadWasm(this.opts.wasmBase);
      const chResp = await fetchChallenge(
        this.opts.endpoint,
        this.opts.siteKey
      );

      // diff 可被 widget 上的 data-diff 覆盖（仅影响 maxIters 上限的预期估算）
      const total = expectedAttempts(chResp.challenge.diff);
      const payloadJson = JSON.stringify(chResp);
      const solver = wasm.create_solver(
        payloadJson,
        BigInt(this.opts.maxIters)
      );

      try {
        const result = await this.solveChunked(solver, total);
        this.updateProgress(100);

        const challenge = JSON.parse(solver.challenge_json());
        const sig = solver.sig();

        const v = await submitVerify(
          this.opts.endpoint,
          challenge,
          sig,
          result.nonce
        );
        if (!v.success) throw new Error('服务端校验失败');

        this.token = v.captcha_token;
        this.setState('success', t(this.opts.lang, 'success'));
        this.opts.onSuccess?.(this.token);

        // 自动填充 target input
        const targetId = this.container.getAttribute('data-target');
        if (targetId) {
          const input = document.getElementById(targetId) as HTMLInputElement;
          if (input) input.value = this.token;
        }

        // token 过期自动重置
        const ttl = v.exp - Date.now();
        if (ttl > 0) {
          this.setTimer(() => {
            if (this.destroyed) return;
            this.token = null;
            this.setState('idle', t(this.opts.lang, 'click_to_verify'));
            this.opts.onExpired?.();
          }, ttl);
        }
      } finally {
        solver.free();
      }
    } catch (err) {
      this.reportError(err instanceof Error ? err : new Error(String(err)));
    }
  }

  private solveChunked(
    solver: ReturnType<WasmModule['create_solver']>,
    expectedTotal: number
  ): Promise<{ nonce: number; attempts: number }> {
    return new Promise((resolve, reject) => {
      const tick = () => {
        if (this.destroyed) {
          reject(new Error('widget 已销毁'));
          return;
        }
        try {
          const r = solver.step(BigInt(CHUNK_SIZE));
          this.updateProgress((Number(r.attempts) / expectedTotal) * 100);

          if (r.found) {
            resolve({ nonce: Number(r.nonce!), attempts: Number(r.attempts) });
          } else if (r.exhausted) {
            reject(new Error('超出最大迭代次数'));
          } else {
            this.setTimer(tick, 0);
          }
        } catch (e) {
          reject(e);
        }
      };
      this.setTimer(tick, 0);
    });
  }

  reset(): void {
    if (this.destroyed || this.state === 'unsupported') return;
    this.token = null;
    this.setState('idle', t(this.opts.lang, 'click_to_verify'));
  }

  getResponse(): string | null {
    return this.token;
  }

  destroy(): void {
    this.destroyed = true;
    for (const id of this.pendingTimers) clearTimeout(id);
    this.pendingTimers.clear();
    this.el.remove();
  }
}
