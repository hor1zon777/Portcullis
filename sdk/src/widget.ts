/** Widget 渲染与状态机（chunked 主线程求解，无 Worker 依赖） */

import { fetchChallenge, submitVerify } from './api';
import { t } from './i18n';
import { injectStyles } from './styles';
import type { CaptchaOptions, CaptchaWidget } from './types';

type State = 'idle' | 'running' | 'success' | 'error';
const CHUNK_SIZE = 5000;

function expectedAttempts(diff: number): number {
  return Math.pow(2, diff);
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

let wasmCache: WasmModule | null = null;

async function loadWasm(wasmBase: string): Promise<WasmModule> {
  if (wasmCache) return wasmCache;
  const mod: WasmModule = await import(
    /* @vite-ignore */ `${wasmBase}/captcha_wasm.js`
  );
  await mod.default();
  mod.init();
  wasmCache = mod;
  return mod;
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
  }

  private render() {
    const wrap = document.createElement('div');
    wrap.className = 'powc-widget powc-idle';
    wrap.setAttribute('data-theme', this.opts.theme);
    wrap.setAttribute('role', 'button');
    wrap.setAttribute('tabindex', '0');

    const check = document.createElement('div');
    check.className = 'powc-check';

    const labelWrap = document.createElement('div');
    labelWrap.className = 'powc-label';

    const label = document.createElement('div');
    label.textContent = t(this.opts.lang, 'click_to_verify');

    const progress = document.createElement('div');
    progress.className = 'powc-progress';
    progress.style.display = 'none';
    const bar = document.createElement('span');
    progress.appendChild(bar);

    const brand = document.createElement('span');
    brand.className = 'powc-brand';
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
      'powc-error'
    );
    this.el.classList.add(`powc-${s}`);
    if (labelText !== undefined) this.label.textContent = labelText;
    this.progress.style.display = s === 'running' ? 'block' : 'none';
    this.bar.style.width = '0%';
  }

  private reportError(err: Error) {
    this.setState('error', t(this.opts.lang, 'failed'));
    this.opts.onError?.(err);
  }

  async start(): Promise<void> {
    if (this.destroyed || this.state === 'running' || this.state === 'success')
      return;
    this.setState('running', t(this.opts.lang, 'verifying'));
    this.token = null;

    try {
      const wasm = await loadWasm(this.opts.wasmBase);
      const chResp = await fetchChallenge(
        this.opts.endpoint,
        this.opts.siteKey
      );
      const total = expectedAttempts(chResp.challenge.diff);

      const payloadJson = JSON.stringify(chResp);
      const solver = wasm.create_solver(
        payloadJson,
        BigInt(this.opts.maxIters)
      );

      try {
        const result = await this.solveChunked(solver, total);
        this.bar.style.width = '100%';

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
          setTimeout(() => {
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
          const pct = Math.min(100, (Number(r.attempts) / expectedTotal) * 100);
          this.bar.style.width = pct.toFixed(1) + '%';

          if (r.found) {
            resolve({ nonce: Number(r.nonce!), attempts: Number(r.attempts) });
          } else if (r.exhausted) {
            reject(new Error('超出最大迭代次数'));
          } else {
            setTimeout(tick, 0);
          }
        } catch (e) {
          reject(e);
        }
      };
      // 第一次 tick 用 setTimeout 确保 UI 已更新
      setTimeout(tick, 0);
    });
  }

  reset(): void {
    if (this.destroyed) return;
    this.token = null;
    this.setState('idle', t(this.opts.lang, 'click_to_verify'));
  }

  getResponse(): string | null {
    return this.token;
  }

  destroy(): void {
    this.destroyed = true;
    this.el.remove();
  }
}
