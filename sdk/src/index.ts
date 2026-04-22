/**
 * PoW CAPTCHA 浏览器 SDK。
 *
 * 公开 API：
 *   import { render } from 'pow-captcha-sdk';
 *   const w = render('#container', { siteKey, endpoint, onSuccess });
 *   w.reset();
 *   w.getResponse();
 *
 * 或 UMD 方式：
 *   <script src="pow-captcha.umd.cjs"></script>
 *   const w = PowCaptcha.render('#container', {...});
 */

import { Widget } from './widget';
import type { CaptchaOptions, CaptchaWidget } from './types';

export type { CaptchaOptions, CaptchaWidget, Challenge } from './types';

function resolveContainer(target: string | HTMLElement): HTMLElement {
  if (typeof target === 'string') {
    const el = document.querySelector(target);
    if (!el) throw new Error(`找不到容器：${target}`);
    return el as HTMLElement;
  }
  return target;
}

/** 渲染验证码 widget，返回控制句柄。 */
export function render(
  target: string | HTMLElement,
  options: CaptchaOptions
): CaptchaWidget {
  const container = resolveContainer(target);

  if (!options.siteKey) throw new Error('CaptchaOptions.siteKey 必填');
  if (!options.endpoint) throw new Error('CaptchaOptions.endpoint 必填');

  const endpoint = options.endpoint.replace(/\/$/, '');

  return new Widget(container, {
    siteKey: options.siteKey,
    endpoint,
    wasmBase: options.wasmBase?.replace(/\/$/, '') ?? `${endpoint}/sdk`,
    theme: options.theme ?? 'light',
    lang: options.lang ?? 'zh-CN',
    maxIters: options.maxIters ?? 10_000_000,
    onSuccess: options.onSuccess,
    onError: options.onError,
    onExpired: options.onExpired,
  });
}

export default { render };
