/**
 * PoW CAPTCHA 自动挂载入口（IIFE 格式）。
 *
 * 使用方式（零 JS 代码）：
 *   <script src="https://captcha.example.com/sdk/pow-captcha.js"
 *           data-site-key="pk_test"></script>
 *   <div data-pow-captcha data-target="captcha_token"></div>
 *   <input type="hidden" name="captcha_token" id="captcha_token" />
 *
 * data 属性：
 *   [data-site-key]  — script 标签上全局设置 site_key
 *   [data-pow-captcha] — 标记待渲染的容器元素
 *   [data-target]    — 成功后自动填充的 input id
 *   [data-theme]     — light(默认) | dark
 *   [data-lang]      — zh-CN(默认) | en-US
 *   [data-callback]  — 全局回调函数名
 *   [data-diff]      — 覆盖客户端 maxIters
 */

import { render, type CaptchaWidget, type CaptchaOptions } from './index';

interface MountedWidget {
  id: string;
  widget: CaptchaWidget;
  container: HTMLElement;
}

const widgets: MountedWidget[] = [];
let idCounter = 0;
let scriptSiteKey = '';
let scriptEndpoint = '';
let scriptWasmBase = '';

function detectScriptConfig() {
  // IIFE 模式：document.currentScript 直接可用
  let script = document.currentScript as HTMLScriptElement | null;

  // module 模式（dev）：currentScript 为 null，按 data-site-key 反查
  if (!script || !script.src) {
    const candidates = document.querySelectorAll<HTMLScriptElement>(
      'script[data-site-key]'
    );
    if (candidates.length > 0) {
      script = candidates[candidates.length - 1];
    }
  }

  if (!script) return;

  scriptSiteKey = script.dataset.siteKey || '';
  scriptEndpoint = script.dataset.endpoint || '';
  scriptWasmBase = script.dataset.wasmBase || '';

  // 未显式提供时，从脚本 src 自动推导
  if (script.src) {
    const url = new URL(script.src);
    if (!scriptEndpoint) scriptEndpoint = url.origin;
    if (!scriptWasmBase) scriptWasmBase = url.href.replace(/\/[^/]+$/, '');
  }
}

function mountElement(el: HTMLElement): MountedWidget | null {
  if (el.dataset.powMounted) return null;

  const siteKey = el.dataset.siteKey || scriptSiteKey;
  if (!siteKey) {
    console.error('[PowCaptcha] 缺少 site_key，请在 <script> 或 <div> 上设置 data-site-key');
    return null;
  }

  const endpoint = el.dataset.endpoint || scriptEndpoint;
  const wasmBase = el.dataset.wasmBase || scriptWasmBase;
  const theme = (el.dataset.theme as CaptchaOptions['theme']) || 'light';
  const lang = (el.dataset.lang as CaptchaOptions['lang']) || 'zh-CN';
  const callbackName = el.dataset.callback;
  const targetId = el.dataset.target;

  const id = `powc-${++idCounter}`;
  el.dataset.powMounted = id;

  const widget = render(el, {
    siteKey,
    endpoint,
    wasmBase,
    theme,
    lang,
    onSuccess(token: string) {
      if (targetId) {
        const input = document.getElementById(targetId) as HTMLInputElement;
        if (input) input.value = token;
      }
      if (callbackName && typeof (window as any)[callbackName] === 'function') {
        (window as any)[callbackName](token);
      }
    },
    onError(err: Error) {
      console.error('[PowCaptcha]', err);
    },
    onExpired() {
      if (targetId) {
        const input = document.getElementById(targetId) as HTMLInputElement;
        if (input) input.value = '';
      }
    },
  });

  const entry = { id, widget, container: el };
  widgets.push(entry);
  return entry;
}

function mountAll() {
  document.querySelectorAll<HTMLElement>('[data-pow-captcha]').forEach(mountElement);
}

// 读取 script 标签配置（必须在 IIFE 执行阶段同步调用，DOMContentLoaded 后 currentScript 为 null）
detectScriptConfig();

// DOM ready 后自动渲染
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', mountAll);
} else {
  mountAll();
}

// 暴露全局 API
const PowCaptcha = {
  render(
    target: string | HTMLElement,
    options: CaptchaOptions
  ): CaptchaWidget {
    return render(target, options);
  },

  /** 重新扫描 DOM，渲染新出现的 [data-pow-captcha] 元素 */
  mount: mountAll,

  /** 通过 id 获取 widget */
  getWidget(id: string): CaptchaWidget | undefined {
    return widgets.find((w) => w.id === id)?.widget;
  },

  /** 获取所有已挂载的 widget id 列表 */
  getWidgetIds(): string[] {
    return widgets.map((w) => w.id);
  },
};

(window as any).PowCaptcha = PowCaptcha;

export default PowCaptcha;
