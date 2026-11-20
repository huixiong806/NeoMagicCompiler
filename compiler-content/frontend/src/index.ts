// =========================================================================
//  Entry point — scans the page for ```riscv blocks and creates playgrounds
// =========================================================================

import './css/editor.css';
import './css/toolbar.css';
import './css/debugger.css';
import './components/riscv-playground';
import { parseRiscvBlock } from './core/parser';
import { ensureLevels, injectGateStyle, markSidebar } from './core/gate';

function initPlaygrounds(): void {
  document.querySelectorAll('code.language-riscv').forEach((codeEl) => {
    const preEl = codeEl.closest('pre');
    if (!preEl || preEl.dataset.riscvInitialized === 'true') return;
    preEl.dataset.riscvInitialized = 'true';

    const parsed = parseRiscvBlock(codeEl.textContent || '');

    if (parsed.error) {
      const errDiv = document.createElement('div');
      errDiv.style.cssText =
        'margin:1.5rem 0;padding:1rem 1.25rem;' +
        'color:var(--fg);background:var(--quote-bg);' +
        'border-left:4px solid #e74c3c;border-radius:6px;' +
        'font-size:0.95rem;';
      errDiv.textContent = `⚠ 关卡配置错误：${parsed.error}`;
      preEl.parentNode!.replaceChild(errDiv, preEl);
      return;
    }

    if (parsed.config) {
      const pg = document.createElement('riscv-playground');
      // Set as JS properties (not attributes) so Lit receives the correct types
      (pg as any).config = parsed.config;
      (pg as any).initialCode = parsed.code;
      preEl.parentNode!.replaceChild(pg, preEl);
    }
    // If no config: pure code block — leave as-is
  });
}

/**
 * 进度门禁：侧边栏里未解锁的关卡标成"还没打开"（点不动）。
 *
 * 只在页面里没有 `riscv` 块时推迟创建编辑器，别的一律照常：
 * 玩家硬要进未解锁的关卡是他的自由（见 core/gate.ts）。
 * mdBook 0.5 的侧边栏是 `<iframe src="toc.html">`，iframe 加载完之后
 * 可能才出现那批链接 —— 顺手把 `load` 也接上（每个 iframe 只接一次）。
 */
function initGate(): void {
  injectGateStyle();
  if (!document.querySelector('code.language-riscv')) {
    initPlaygrounds(); // 非关卡页：没必要等门禁数据
  }
  void ensureLevels().then(() => {
    markSidebar();
    // 关卡页：编辑器照常创建（只是等这一份数据回来）
    initPlaygrounds();
  });
  document.querySelectorAll('iframe').forEach((f) => {
    f.addEventListener('load', () => markSidebar(), { once: true });
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initGate);
} else {
  initGate();
}
