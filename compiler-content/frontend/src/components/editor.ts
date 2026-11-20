// =========================================================================
//  Zone 1 — 代码编辑器（gutter + lines + 键盘处理）
//
//  纯编辑器：不含工具栏、控制台、调试面板。宿主元素里需要有
//  `.riscv-gutter` 与 `.riscv-lines` 两个容器。
//
//  玩家写的不是"一整个 textarea"，而是**一行一个 textarea**：
//  这样才能做"整行只读""隐藏行""行内灰色提示"这类关卡约束。
// =========================================================================

import { applyConstraints, toEditorConstraints } from '../core/constraints';
import { restoreDraft, createAutoSave } from '../core/storage';
import type { EditorConfig } from '../core/types';

export interface EditorHost {
  config?: EditorConfig;
  initialCode: string;
}

export interface EditorHandle {
  /** 把当前所有行拼成一份源码 */
  collectCode(): string;
  getLineEls(): HTMLTextAreaElement[];
  /** 换一份代码（后端把关卡初始代码送过来时用） */
  setCode(code: string): void;
  /** 立刻把草稿写进 localStorage */
  flush(): void;
  schedule(): void;
}

/**
 * 在 host 元素里建出编辑器 DOM，返回控制句柄。
 * 在 Lit 组件的 `firstUpdated()` 里调用。
 */
export function createEditor(
  host: HTMLElement,
  config: EditorConfig,
  initialCode: string,
): EditorHandle | null {
  const gutterEl = host.querySelector('.riscv-gutter') as HTMLElement | null;
  const linesEl = host.querySelector('.riscv-lines') as HTMLElement | null;
  if (!gutterEl || !linesEl) return null;

  const lineEls: HTMLTextAreaElement[] = [];
  const auto = createAutoSave(config.level, () => collectCode());
  const flush = () => auto.flush();
  const schedule = () => auto.schedule();
  window.addEventListener('beforeunload', flush);

  // ── 建行 ────────────────────────────────────────────────────────────

  function buildLines(code: string, restore: boolean): void {
    const codeLines = (code || '').split('\n');

    gutterEl!.innerHTML = '';
    linesEl!.innerHTML = '';
    lineEls.length = 0;

    for (let i = 0; i < codeLines.length; i++) {
      const gl = document.createElement('div');
      gl.className = 'riscv-gutter-line';
      gl.setAttribute('data-line', String(i + 1));
      gl.textContent = String(i + 1);
      gutterEl!.appendChild(gl);

      const ta = document.createElement('textarea');
      ta.className = 'riscv-line';
      ta.setAttribute('data-line', String(i + 1));
      ta.spellcheck = false;
      ta.wrap = 'off';
      ta.rows = 1;
      ta.value = codeLines[i];
      ta.addEventListener('keydown', (e) => onKeydown(e, ta));
      ta.addEventListener('input', () => autoResize(ta));
      ta.addEventListener('blur', flush);
      linesEl!.appendChild(ta);
      lineEls.push(ta);
      autoResize(ta);
    }

    // ── 关卡约束 ──────────────────────────────────────────────────────
    // 后端给的是 readonlyPrefix/readonlySuffix（"前 N 行/后 N 行"这种**区域**写法），
    // 前端编辑器要的是具体行号列表，所以这里换算一次。
    applyConstraints(toEditorConstraints(config), lineEls, config);

    // ── 草稿 ──────────────────────────────────────────────────────────
    if (restore) {
      const draft = restoreDraft(config.level);
      if (draft) {
        const draftLines = draft.split('\n');
        for (let i = 0; i < Math.min(draftLines.length, lineEls.length); i++) {
          if (!lineEls[i].readOnly) lineEls[i].value = draftLines[i];
        }
        lineEls.forEach((t) => autoResize(t));
      }
    }
  }

  buildLines(initialCode, true);

  // ── 滚动同步 ────────────────────────────────────────────────────────

  linesEl.addEventListener('scroll', () => {
    gutterEl!.scrollTop = linesEl.scrollTop;
  });

  // ── 对外接口 ────────────────────────────────────────────────────────

  function collectCode(): string {
    return lineEls.map((t) => t.value).join('\n');
  }

  function getLineEls(): HTMLTextAreaElement[] {
    return lineEls;
  }

  /** 换一份代码：不加草稿、不覆盖约束（用于"载入关卡"）。 */
  function setCode(code: string): void {
    buildLines(code, false);
  }

  return { collectCode, getLineEls, setCode, flush, schedule };

  // ===================================================================
  //  内部小工具
  // ===================================================================

  function autoResize(ta: HTMLTextAreaElement): void {
    ta.style.height = 'auto';
    const h = ta.scrollHeight + 'px';
    ta.style.height = h;
    // 行号栏要跟着行高走，否则对不齐
    const lineNo = ta.getAttribute('data-line');
    if (lineNo && gutterEl) {
      const gl = gutterEl.querySelector(
        `.riscv-gutter-line[data-line="${lineNo}"]`,
      ) as HTMLElement | null;
      if (gl) gl.style.height = h;
    }
  }

  function insertLine(afterIdx: number, content: string): void {
    const ta = document.createElement('textarea');
    ta.className = 'riscv-line';
    ta.spellcheck = false;
    ta.wrap = 'off';
    ta.rows = 1;
    ta.value = content;
    ta.addEventListener('keydown', (e) => onKeydown(e, ta));
    ta.addEventListener('input', () => autoResize(ta));
    ta.addEventListener('blur', flush);

    const ref = lineEls[afterIdx];
    if (ref?.parentNode) {
      ref.parentNode.insertBefore(ta, ref.nextSibling);
    } else {
      linesEl!.appendChild(ta);
    }
    lineEls.splice(afterIdx + 1, 0, ta);

    const gl = document.createElement('div');
    gl.className = 'riscv-gutter-line';
    const refGl = gutterEl!.querySelector(`.riscv-gutter-line[data-line="${afterIdx + 1}"]`);
    if (refGl?.parentNode) {
      refGl.parentNode.insertBefore(gl, refGl.nextSibling);
    } else {
      gutterEl!.appendChild(gl);
    }

    renumberAll();
    autoResize(ta);
    ta.focus();
    ta.setSelectionRange(0, 0);
    schedule();
  }

  function removeLine(idx: number): void {
    const el = lineEls[idx];
    if (!el) return;
    const gl = gutterEl?.querySelector(`.riscv-gutter-line[data-line="${idx + 1}"]`);
    if (gl) gl.remove();
    el.remove();
    lineEls.splice(idx, 1);
    renumberAll();
    schedule();
  }

  function renumberAll(): void {
    for (let i = 0; i < lineEls.length; i++) {
      lineEls[i].setAttribute('data-line', String(i + 1));
    }
    if (gutterEl) {
      const gls = gutterEl.querySelectorAll('.riscv-gutter-line');
      gls.forEach((gl, i) => {
        gl.setAttribute('data-line', String(i + 1));
        gl.textContent = String(i + 1);
      });
    }
  }

  // ===================================================================
  //  键盘处理 —— 行号每次都从 lineEls 现算，避免增删行之后错位
  // ===================================================================

  function onKeydown(e: KeyboardEvent, ta: HTMLTextAreaElement): void {
    const idx = lineEls.indexOf(ta);
    if (idx === -1) return;

    const key = e.key;
    const ss = ta.selectionStart;
    const se = ta.selectionEnd;
    const val = ta.value;

    // 只读行：只允许导航与复制
    if (ta.readOnly) {
      const ok =
        ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(key) ||
        ((e.ctrlKey || e.metaKey) && (key === 'c' || key === 'a'));
      if (!ok) {
        e.preventDefault();
        return;
      }
    }

    // Tab → 四个空格
    if (key === 'Tab' && !e.shiftKey) {
      e.preventDefault();
      ta.setRangeText('    ', ss, se, 'end');
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }

    // Enter → 拆行
    if (key === 'Enter') {
      e.preventDefault();
      const after = val.substring(se);
      ta.value = val.substring(0, ss);
      insertLine(idx, after);
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }

    // 行首 Backspace → 并入上一行
    if (key === 'Backspace' && ss === 0 && se === 0 && idx > 0) {
      const prev = lineEls[idx - 1];
      if (!prev.readOnly) {
        e.preventDefault();
        const plen = prev.value.length;
        prev.value += val;
        prev.setSelectionRange(plen, plen);
        prev.focus();
        prev.dispatchEvent(new Event('input', { bubbles: true }));
        removeLine(idx);
      }
      return;
    }

    // 行尾 Delete → 吞掉下一行
    if (key === 'Delete' && ss === val.length && se === val.length && idx < lineEls.length - 1) {
      const next = lineEls[idx + 1];
      if (!next.readOnly) {
        e.preventDefault();
        ta.value = val + next.value;
        ta.setSelectionRange(val.length, val.length);
        ta.dispatchEvent(new Event('input', { bubbles: true }));
        removeLine(idx + 1);
      }
      return;
    }

    // 上下箭头：保持列号（超出就贴到行尾）
    if (key === 'ArrowUp' && idx > 0) {
      e.preventDefault();
      const p = lineEls[idx - 1];
      const col = Math.min(ss, p.value.length);
      p.focus();
      p.setSelectionRange(col, col);
      return;
    }

    if (key === 'ArrowDown' && idx < lineEls.length - 1) {
      e.preventDefault();
      const n = lineEls[idx + 1];
      const col = Math.min(se, n.value.length);
      n.focus();
      n.setSelectionRange(col, col);
    }
  }
}
