// =========================================================================
//  Zone 3 — createConsole(host) → { clear, append, appendHTML }
//  纯 DOM 工厂，无框架依赖。host 内需有 .riscv-console-body 元素。
// =========================================================================

import type { ConsoleLevel } from '../core/types';

export interface ConsoleHandle {
  clear(): void;
  append(level: ConsoleLevel, msg: string): void;
  appendHTML(html: string): void;
}

export function createConsole(host: HTMLElement): ConsoleHandle | null {
  const body = host.querySelector('.riscv-console-body');
  if (!body) return null;

  function clear(): void {
    body!.innerHTML = '';
  }

  function append(level: ConsoleLevel, msg: string): void {
    const entry = document.createElement('div');
    entry.className = `riscv-console-entry riscv-console-entry--${level}`;
    entry.textContent = msg;
    body!.appendChild(entry);
    body!.scrollTop = body!.scrollHeight;
  }

  function appendHTML(html: string): void {
    const entry = document.createElement('div');
    entry.className = 'riscv-console-entry';
    entry.innerHTML = html;
    body!.appendChild(entry);
    body!.scrollTop = body!.scrollHeight;
  }

  // Wire clear button
  const clearBtn = host.querySelector('.riscv-console-clear');
  if (clearBtn) clearBtn.addEventListener('click', clear);

  return { clear, append, appendHTML };
}
