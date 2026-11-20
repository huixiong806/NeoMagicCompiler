// =========================================================================
//  Zone 2b — createDebugger(host)
//  《机器状态》面板：PC 摘要 + 寄存器网格 (8×4) + 内存监视
//
//  数据由后端驱动：每次 `debug_step` / `debug_step_back` / `debug_continue`
//  都会带回一份 MachineSnapshot（+ 这一步改了什么），这里只负责渲染。
// =========================================================================

import type { MachineSnapshot, RegisterSnapshot } from '../core/types';

export interface DebuggerHandle {
  show(): void;
  hide(): void;
  /** Full render from a backend snapshot. */
  setSnapshot(snap: MachineSnapshot): void;
  /**
   * Render a step result: same as setSnapshot, but flashes the registers and
   * lists the memory bytes that changed in this very step.
   */
  renderStep(
    snap: MachineSnapshot,
    regChanges?: RegisterSnapshot,
    memChanges?: Record<string, number>,
  ): void;
  reset(): void;
}

function fmt(v: number): string {
  return '0x' + (v >>> 0).toString(16).padStart(8, '0');
}

/** Turns a pause-kind string into something a player can read. */
function reasonClass(kind: string): string {
  if (kind === 'error' || kind === 'limit') return 'riscv-pc-reason--bad';
  if (kind === 'exit') return 'riscv-pc-reason--ok';
  return '';
}

export function createDebugger(host: HTMLElement): DebuggerHandle | null {
  const panel = host.querySelector('.riscv-debug-panel') as HTMLElement | null;
  if (!panel) return null;

  const pcSummary = panel.querySelector('.riscv-pc-summary') as HTMLElement | null;
  const regTable = panel.querySelector('.riscv-reg-grid') as HTMLElement | null;
  const memAuto = panel.querySelector('.riscv-memory-auto') as HTMLElement | null;
  const memAddrInput = panel.querySelector('.riscv-memory-addr') as HTMLInputElement | null;

  const regCells: HTMLElement[] = [];
  const regValueSpans: HTMLElement[] = [];
  let regTimers: Array<ReturnType<typeof setTimeout> | undefined> = new Array(32).fill(undefined);

  // Cache the 32 register cells
  if (regTable) {
    const tds = regTable.querySelectorAll('td[data-reg]');
    tds.forEach((td) => {
      const idx = parseInt((td as HTMLElement).dataset.reg ?? '0', 10);
      regCells[idx] = td as HTMLElement;
      regValueSpans[idx] = td.querySelector('.riscv-reg-value') as HTMLElement;
    });
  }

  let currentRegs: RegisterSnapshot = {};

  function renderRegs(regs: RegisterSnapshot, highlight: Set<number> = new Set()): void {
    currentRegs = { ...regs };
    for (let i = 0; i < 32; i++) {
      if (!regValueSpans[i]) continue;
      regValueSpans[i].textContent = fmt(regs[`x${i}`] ?? 0);
      if (highlight.has(i)) {
        if (regTimers[i] !== undefined) clearTimeout(regTimers[i]);
        regCells[i]?.classList.add('riscv-reg--changed');
        regTimers[i] = setTimeout(() => {
          regCells[i]?.classList.remove('riscv-reg--changed');
          regTimers[i] = undefined;
        }, 500);
      }
    }
  }

  function renderSummary(snap: MachineSnapshot): void {
    if (!pcSummary) return;
    const line = snap.pcLine > 0 ? `第 ${snap.pcLine} 行` : '（引导代码里）';
    pcSummary.innerHTML =
      `<span><span class="riscv-pc-label">PC</span> ` +
      `<span class="riscv-pc-value">${fmt(snap.pc)}</span></span>` +
      `<span class="riscv-pc-line">${line}</span>` +
      `<span class="riscv-pc-reason ${reasonClass(snap.pauseKind)}">${snap.pauseDetail}</span>` +
      `<span class="riscv-pc-count">已执行 ${snap.instructions} 条</span>`;
  }

  function renderMemChanges(memChanges?: Record<string, number>): void {
    if (!memAuto) return;
    const entries = Object.entries(memChanges ?? {});
    if (entries.length === 0) {
      memAuto.innerHTML = '';
      return;
    }
    let html = '<div class="riscv-memory-auto-header">── 上一步写过的字节 ──</div>';
    for (const [addr, byte] of entries) {
      const hex = (byte & 0xff).toString(16).padStart(2, '0').toUpperCase();
      html += `<div class="riscv-memory-entry riscv-memory-entry--auto">
        <span class="riscv-memory-icon">W</span>
        <span class="riscv-memory-addr-text">0x${Number(addr).toString(16).toUpperCase()}</span>
        <span class="riscv-memory-value">${hex}</span></div>`;
    }
    memAuto.innerHTML = html;
  }

  // ── Public API ─────────────────────────────────────────────────────

  function show(): void {
    panel!.style.display = 'block';
  }

  function hide(): void {
    panel!.style.display = 'none';
  }

  function setSnapshot(snap: MachineSnapshot): void {
    renderSummary(snap);
    renderRegs(snap.regs);
    renderMemChanges();
  }

  function renderStep(
    snap: MachineSnapshot,
    regChanges?: RegisterSnapshot,
    memChanges?: Record<string, number>,
  ): void {
    renderSummary(snap);

    const highlight = new Set<number>();
    if (regChanges) {
      const nextRegs = { ...currentRegs };
      for (const [k, v] of Object.entries(regChanges)) {
        const idx = parseInt(k.replace('x', ''), 10);
        if (idx >= 0 && idx < 32) {
          nextRegs[k] = v;
          highlight.add(idx);
        }
      }
      renderRegs(nextRegs, highlight);
    } else {
      renderRegs(snap.regs);
    }

    renderMemChanges(memChanges);
  }

  function reset(): void {
    currentRegs = {};
    renderRegs({});
    if (pcSummary) {
      pcSummary.innerHTML =
        '<span><span class="riscv-pc-label">PC</span> ' +
        '<span class="riscv-pc-value">0x00000000</span></span>' +
        '<span class="riscv-pc-reason">等待开始</span>' +
        '<span class="riscv-pc-count">已执行 0 条</span>';
    }
    if (memAuto) memAuto.innerHTML = '';
    regTimers.forEach((t) => {
      if (t) clearTimeout(t);
    });
    regTimers = new Array(32).fill(undefined);
  }

  // ── Manual memory watch ────────────────────────────────────────────

  function addWatch(addr: string): void {
    if (!addr) return;
    const list = panel!.querySelector('.riscv-memory-manual');
    if (!list) return;
    const entry = document.createElement('div');
    entry.className = 'riscv-memory-entry';
    entry.innerHTML =
      `<span class="riscv-memory-icon">W</span>` +
      `<span class="riscv-memory-addr-text">${addr}</span>` +
      `<span class="riscv-memory-value">--</span>`;
    list.appendChild(entry);
  }

  if (memAddrInput) {
    memAddrInput.addEventListener('keydown', (e) => {
      if (e.key !== 'Enter') return;
      const addr = memAddrInput.value.trim();
      memAddrInput.value = '';
      addWatch(addr);
    });
  }

  const addBtn = panel.querySelector('.riscv-memory-add-btn');
  if (addBtn && memAddrInput) {
    addBtn.addEventListener('click', () => {
      const addr = memAddrInput.value.trim();
      memAddrInput.value = '';
      addWatch(addr);
    });
  }

  return { show, hide, setSnapshot, renderStep, reset };
}
