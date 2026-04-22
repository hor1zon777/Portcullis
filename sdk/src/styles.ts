/** Widget 内联样式 */

export const WIDGET_CSS = `
.powc-widget {
  display: inline-flex;
  align-items: center;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto,
    'PingFang SC', 'Microsoft YaHei', sans-serif;
  font-size: 14px;
  width: 300px;
  max-width: 100%;
  padding: 14px 16px;
  box-sizing: border-box;
  border: 1px solid var(--powc-border, #d0d7de);
  border-radius: 10px;
  background: var(--powc-bg, #f6f8fa);
  color: var(--powc-fg, #1f2328);
  user-select: none;
  gap: 12px;
  transition: all 0.2s ease;
}
.powc-widget[data-theme='dark'] {
  --powc-border: #30363d;
  --powc-bg: #161b22;
  --powc-fg: #e6edf3;
}
.powc-widget.powc-idle:hover {
  cursor: pointer;
  background: var(--powc-bg-hover, #eaeef2);
  border-color: var(--powc-border-hover, #bbc0c5);
  box-shadow: 0 2px 8px rgba(0,0,0,0.06);
}
.powc-widget[data-theme='dark'].powc-idle:hover {
  --powc-bg-hover: #1c2128;
  --powc-border-hover: #444c56;
}
/* ── 勾选框 ── */
.powc-check {
  width: 24px;
  height: 24px;
  flex: 0 0 24px;
  border: 2px solid var(--powc-check-border, #8c959f);
  border-radius: 6px;
  background: #fff;
  position: relative;
  box-sizing: border-box;
  transition: all 0.2s ease;
}
.powc-widget[data-theme='dark'] .powc-check {
  background: rgba(255,255,255,0.06);
  --powc-check-border: #484f58;
}
.powc-widget.powc-running .powc-check {
  border-color: #0969da;
  border-top-color: #0969da;
  border-right-color: transparent;
  border-bottom-color: transparent;
  border-left-color: transparent;
  border-radius: 50%;
  animation: powc-spin 0.7s linear infinite;
}
.powc-widget.powc-success .powc-check {
  background: #1a7f37;
  border-color: #1a7f37;
  animation: powc-pop 0.3s ease;
}
.powc-widget.powc-success .powc-check::after {
  content: '';
  position: absolute;
  left: 6px;
  top: 2px;
  width: 6px;
  height: 11px;
  border: solid #fff;
  border-width: 0 2.5px 2.5px 0;
  transform: rotate(45deg);
}
.powc-widget.powc-error .powc-check {
  border-color: #cf222e;
  animation: powc-shake 0.4s ease;
}
/* ── 标签区 ── */
.powc-label {
  flex: 1 1 auto;
  font-weight: 500;
}
.powc-progress {
  display: block;
  margin-top: 6px;
  height: 4px;
  width: 100%;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.07);
  overflow: hidden;
}
.powc-widget[data-theme='dark'] .powc-progress {
  background: rgba(255, 255, 255, 0.08);
}
.powc-progress > span {
  display: block;
  height: 100%;
  width: 0;
  background: linear-gradient(90deg, #0969da, #1f6feb);
  border-radius: 4px;
  transition: width 0.25s ease;
}
/* ── 品牌区（右侧竖排）── */
.powc-brand {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  flex: 0 0 auto;
  opacity: 0.5;
  transition: opacity 0.2s;
}
.powc-widget:hover .powc-brand {
  opacity: 0.75;
}
.powc-brand svg {
  width: 20px;
  height: 20px;
}
.powc-brand-text {
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.5px;
  text-transform: uppercase;
  line-height: 1;
  white-space: nowrap;
}
.powc-brand-ver {
  font-size: 9px;
  opacity: 0.7;
  line-height: 1;
}
/* ── 动画 ── */
@keyframes powc-spin {
  to { transform: rotate(360deg); }
}
@keyframes powc-pop {
  0% { transform: scale(0.8); }
  50% { transform: scale(1.15); }
  100% { transform: scale(1); }
}
@keyframes powc-shake {
  0%, 100% { transform: translateX(0); }
  20%, 60% { transform: translateX(-3px); }
  40%, 80% { transform: translateX(3px); }
}
/* ── 状态 ── */
.powc-widget.powc-unsupported {
  opacity: 0.6;
  cursor: not-allowed;
}
.powc-widget.powc-unsupported .powc-check {
  border-color: #8c959f;
}
/* ── 响应式 ── */
@media (max-width: 340px) {
  .powc-widget {
    padding: 10px 12px;
    gap: 8px;
    font-size: 13px;
  }
  .powc-brand { display: none; }
}
`;

let injected = false;
export function injectStyles(): void {
  if (injected || typeof document === 'undefined') return;
  const style = document.createElement('style');
  style.setAttribute('data-powc', '');
  style.textContent = WIDGET_CSS;
  document.head.appendChild(style);
  injected = true;
}
