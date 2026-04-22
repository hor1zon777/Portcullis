/** Widget 渲染与状态机（chunked 主线程求解，无 Worker 依赖） */

import { fetchChallenge, submitVerify } from './api';
import { t } from './i18n';
import { injectStyles } from './styles';
import type { CaptchaOptions, CaptchaWidget } from './types';

type State = 'idle' | 'running' | 'success' | 'error' | 'unsupported';
const CHUNK_SIZE = 5000;
const SOLVE_TIMEOUT_MS = 60_000;

function expectedAttempts(diff: number): number {
  return Math.pow(2, diff);
}

function wasmSupported(): boolean {
  return (
    typeof WebAssembly === 'object' &&
    typeof WebAssembly.instantiate === 'function'
  );
}

function bigIntSupported(): boolean {
  try {
    return typeof BigInt === 'function' && BigInt(1) + BigInt(0) === BigInt(1);
  } catch {
    return false;
  }
}

type SolverInstance = {
  step: (chunkSize: number | bigint) => {
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

type WasmModule = {
  default: (input?: unknown) => Promise<unknown>;
  init: () => void;
  create_solver: (
    payloadJson: string,
    hardLimit: number | bigint
  ) => SolverInstance;
};

const wasmCache = new Map<string, Promise<WasmModule>>();

async function loadWasm(wasmBase: string): Promise<WasmModule> {
  const cached = wasmCache.get(wasmBase);
  if (cached) return cached;
  const promise = (async () => {
    const url = `${wasmBase}/captcha_wasm.js`;
    let mod: WasmModule;
    try {
      mod = await import(/* @vite-ignore */ url);
    } catch (e) {
      // 动态 import 失败时尝试 script 注入方式
      console.warn('[Portcullis] dynamic import 失败，尝试 script 加载:', e);
      mod = await loadWasmViaScript(url);
    }
    await mod.default();
    mod.init();
    return mod;
  })();
  wasmCache.set(wasmBase, promise);
  try {
    return await promise;
  } catch (e) {
    wasmCache.delete(wasmBase);
    throw e;
  }
}

/** 兼容不支持动态 import() 的移动端浏览器 */
function loadWasmViaScript(url: string): Promise<WasmModule> {
  return new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = url;
    script.onload = () => {
      // wasm-bindgen web target 会挂到 wasm_bindgen 全局
      const g = globalThis as unknown as Record<string, unknown>;
      if (g.wasm_bindgen) {
        resolve(g.wasm_bindgen as unknown as WasmModule);
      } else {
        reject(new Error('WASM 模块加载失败'));
      }
    };
    script.onerror = () => reject(new Error('WASM 脚本加载失败: ' + url));
    document.head.appendChild(script);
  });
}

/** 安全的 BigInt 转换，不支持时 fallback 到 Number */
function toBigIntSafe(n: number): number | bigint {
  if (bigIntSupported()) return BigInt(n);
  return n;
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

    const brand = document.createElement('div');
    brand.className = 'powc-brand';
    brand.setAttribute('aria-hidden', 'true');
    brand.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2L3 7v6c0 5.5 3.8 10.7 9 12 5.2-1.3 9-6.5 9-12V7l-9-5z"/><path d="m9 12 2 2 4-4"/></svg><span class="powc-brand-text">Portcullis</span><span class="powc-brand-ver">v1.1.1</span>`;

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
    console.error('[Portcullis]', err);
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
      // 1. 加载 WASM
      const wasm = await loadWasm(this.opts.wasmBase);

      // 2. 获取挑战
      const chResp = await fetchChallenge(
        this.opts.endpoint,
        this.opts.siteKey
      );

      const total = expectedAttempts(chResp.challenge.diff);
      const payloadJson = JSON.stringify(chResp);

      // 3. 创建求解器（Argon2 base hash 同步计算）
      // 用 setTimeout 让 UI 先更新再开始重计算
      await new Promise((r) => setTimeout(r, 50));
      const solver = wasm.create_solver(
        payloadJson,
        toBigIntSafe(this.opts.maxIters)
      );

      try {
        // 4. 分块求解（带超时）
        const result = await this.solveChunked(solver, total);
        this.updateProgress(100);

        // 5. 提交验证
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

        const targetId = this.container.getAttribute('data-target');
        if (targetId) {
          const input = document.getElementById(targetId) as HTMLInputElement;
          if (input) input.value = this.token;
        }

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
    solver: SolverInstance,
    expectedTotal: number
  ): Promise<{ nonce: number; attempts: number }> {
    return new Promise((resolve, reject) => {
      const startTime = Date.now();
      const chunkVal = toBigIntSafe(CHUNK_SIZE);

      const tick = () => {
        if (this.destroyed) {
          reject(new Error('widget 已销毁'));
          return;
        }
        if (Date.now() - startTime > SOLVE_TIMEOUT_MS) {
          reject(new Error('求解超时（60s），请刷新重试'));
          return;
        }
        try {
          const r = solver.step(chunkVal);
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
