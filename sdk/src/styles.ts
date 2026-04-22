/** Widget 内联样式。运行时通过 <style> 注入一次 */

export const WIDGET_CSS = `
.powc-widget {
  display: inline-flex;
  align-items: center;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto,
    'PingFang SC', 'Microsoft YaHei', sans-serif;
  font-size: 14px;
  width: 300px;
  padding: 12px 14px;
  box-sizing: border-box;
  border: 1px solid var(--powc-border, #d0d7de);
  border-radius: 6px;
  background: var(--powc-bg, #f6f8fa);
  color: var(--powc-fg, #1f2328);
  user-select: none;
  gap: 12px;
  transition: background 0.15s ease;
}
.powc-widget[data-theme='dark'] {
  --powc-border: #30363d;
  --powc-bg: #0d1117;
  --powc-fg: #e6edf3;
}
.powc-widget.powc-idle:hover {
  cursor: pointer;
  background: var(--powc-bg-hover, #eaeef2);
}
.powc-widget[data-theme='dark'].powc-idle:hover {
  --powc-bg-hover: #161b22;
}
.powc-check {
  width: 22px;
  height: 22px;
  flex: 0 0 22px;
  border: 2px solid var(--powc-border, #8c959f);
  border-radius: 3px;
  background: #fff;
  position: relative;
  box-sizing: border-box;
}
.powc-widget[data-theme='dark'] .powc-check {
  background: transparent;
}
.powc-widget.powc-running .powc-check {
  border-top-color: #0969da;
  border-right-color: transparent;
  border-bottom-color: transparent;
  border-left-color: transparent;
  border-radius: 50%;
  animation: powc-spin 0.8s linear infinite;
}
.powc-widget.powc-success .powc-check {
  background: #2da44e;
  border-color: #2da44e;
}
.powc-widget.powc-success .powc-check::after {
  content: '';
  position: absolute;
  left: 5px;
  top: 1px;
  width: 6px;
  height: 12px;
  border: solid #fff;
  border-width: 0 2px 2px 0;
  transform: rotate(45deg);
}
.powc-widget.powc-error .powc-check {
  border-color: #cf222e;
}
.powc-label {
  flex: 1 1 auto;
}
.powc-progress {
  display: block;
  margin-top: 4px;
  height: 4px;
  width: 100%;
  border-radius: 2px;
  background: rgba(0, 0, 0, 0.08);
  overflow: hidden;
}
.powc-widget[data-theme='dark'] .powc-progress {
  background: rgba(255, 255, 255, 0.1);
}
.powc-progress > span {
  display: block;
  height: 100%;
  width: 0;
  background: #0969da;
  transition: width 0.2s ease;
}
.powc-brand {
  font-size: 10px;
  opacity: 0.55;
  letter-spacing: 0.3px;
}
@keyframes powc-spin {
  to { transform: rotate(360deg); }
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
