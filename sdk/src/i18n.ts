type Lang = 'zh-CN' | 'en-US';

interface Strings {
  click_to_verify: string;
  verifying: string;
  success: string;
  failed: string;
  expired: string;
  powered_by: string;
  unsupported: string;
  aria_widget_label: string;
}

const dict: Record<Lang, Strings> = {
  'zh-CN': {
    click_to_verify: '我不是机器人',
    verifying: '正在验证...',
    success: '验证通过',
    failed: '验证失败，请重试',
    expired: '已过期，请重新验证',
    powered_by: 'PoW 验证',
    unsupported: '浏览器不兼容',
    aria_widget_label: 'PoW 验证码，点击验证',
  },
  'en-US': {
    click_to_verify: "I'm not a robot",
    verifying: 'Verifying...',
    success: 'Verified',
    failed: 'Failed, please retry',
    expired: 'Expired, please retry',
    powered_by: 'PoW CAPTCHA',
    unsupported: 'Browser not supported',
    aria_widget_label: 'PoW CAPTCHA, click to verify',
  },
};

export function t(lang: Lang, key: keyof Strings): string {
  return dict[lang][key];
}
