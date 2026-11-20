// =========================================================================
//  Zone 2 — createToolbar(host, config, callbacks)
//  双模式工具栏：normal (运行/调试/提交/☰菜单) ↔ debug (继续/单步/重置/退出)
// =========================================================================

import type { ToolbarConfig } from '../core/types';

export interface ToolbarCallbacks {
  onRun(): void;
  onDebug(): void;
  onSubmit(): void;
  onLoad(): void;
  onSave(): void;
  // debug mode
  onContinue(): void;
  onStep(): void;
  onStepBack(): void;
  onReset(): void;
  onQuitDebug(): void;
}

export interface ToolbarHandle {
  setMode(mode: 'normal' | 'debug'): void;
}

export function createToolbar(
  host: HTMLElement,
  cfg: ToolbarConfig | undefined,
  cb: ToolbarCallbacks,
): ToolbarHandle | null {
  const tb = host.querySelector('.riscv-toolbar') as HTMLElement | null;
  if (!tb) return null;

  const normalGroup = tb.querySelector('.riscv-toolbar--normal') as HTMLElement | null;
  const debugGroup = tb.querySelector('.riscv-toolbar--debug') as HTMLElement | null;

  // ☰ menu
  const dropdown = tb.querySelector('.riscv-menu-dropdown') as HTMLElement | null;
  let menuOpen = false;

  function closeMenu() {
    menuOpen = false;
    if (dropdown) dropdown.style.display = 'none';
  }

  function toggleMenu() {
    menuOpen = !menuOpen;
    if (!dropdown) return;
    dropdown.style.display = menuOpen ? 'block' : 'none';
    if (menuOpen) {
      setTimeout(() => document.addEventListener('click', onDocClick), 0);
    }
  }

  function onDocClick(e: MouseEvent) {
    if (!dropdown || !menuOpen) return;
    if (!dropdown.contains(e.target as Node) && (e.target as HTMLElement)?.getAttribute('data-action') !== 'menu') {
      closeMenu();
      document.removeEventListener('click', onDocClick);
    }
  }

  // Click delegation
  tb.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('[data-action]');
    if (!btn) return;
    const action = btn.getAttribute('data-action')!;
    switch (action) {
      case 'run':        cb.onRun(); break;
      case 'debug':      cb.onDebug(); break;
      case 'submit':     cb.onSubmit(); break;
      case 'load':       closeMenu(); cb.onLoad(); break;
      case 'save':       closeMenu(); cb.onSave(); break;
      case 'menu':       toggleMenu(); break;
      case 'continue':   cb.onContinue(); break;
      case 'step':       cb.onStep(); break;
      case 'step-back':  cb.onStepBack(); break;
      case 'reset':      cb.onReset(); break;
      case 'quit-debug': cb.onQuitDebug(); break;
    }
  });

  return {
    setMode(mode: 'normal' | 'debug') {
      if (normalGroup) normalGroup.style.display = mode === 'normal' ? '' : 'none';
      if (debugGroup) debugGroup.style.display = mode === 'debug' ? '' : 'none';
    },
  };
}
