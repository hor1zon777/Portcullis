/** 浏览器 SDK 使用的共享类型 */

/** 服务端 /challenge 端点返回体 */
export interface ChallengeResponse {
  success: boolean;
  challenge: Challenge;
  sig: string;
}

/** PoW 挑战结构（与 Rust 端保持对齐） */
export interface Challenge {
  id: string;
  /** base64 标准编码的 16 字节盐 */
  salt: string;
  diff: number;
  exp: number;
  site_key: string;
}

/** 服务端 /verify 端点返回体 */
export interface VerifyResponse {
  success: boolean;
  captcha_token: string;
  exp: number;
}

/** 调用方初始化 widget 的配置 */
export interface CaptchaOptions {
  /** 站点公钥，从服务端获得 */
  siteKey: string;
  /** 验证服务基地址，例如 `https://captcha.example.com` */
  endpoint: string;
  /** WASM 资源前缀（默认与 endpoint 同源），指向 `captcha_wasm.js` 所在目录 */
  wasmBase?: string;
  /** 主题 */
  theme?: 'light' | 'dark';
  /** 语言 */
  lang?: 'zh-CN' | 'en-US';
  /** 求解最大迭代次数，默认 1000 万 */
  maxIters?: number;
  /** 成功回调，参数为 captcha_token */
  onSuccess?: (token: string) => void;
  /** 失败回调 */
  onError?: (error: Error) => void;
  /** token 过期回调 */
  onExpired?: () => void;
}

/** 公开的 widget 句柄 */
export interface CaptchaWidget {
  reset(): void;
  getResponse(): string | null;
  destroy(): void;
}

/** Worker 内部消息：主线程 → Worker */
export type WorkerRequest =
  | { type: 'init'; wasmBase: string }
  | {
      type: 'solve';
      payloadJson: string;
      maxIters: number;
      reportInterval: number;
    };

/** Worker 内部消息：Worker → 主线程 */
export type WorkerResponse =
  | { type: 'ready' }
  | { type: 'progress'; attempts: number }
  | {
      type: 'solved';
      nonce: number;
      hash: string;
      attempts: number;
      elapsedMs: number;
    }
  | { type: 'error'; message: string };
