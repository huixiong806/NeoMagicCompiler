// =========================================================================
//  <riscv-playground> — top-level container (Light DOM)
//
//  Zone 1  → editor.ts    (createEditor)
//  Zone 2  → toolbar.ts   (createToolbar)
//  Zone 2b → debugger.ts  (createDebugger)
//  Zone 3  → console.ts   (createConsole)
//
//  All real work happens in the Rust backend; this component only renders the
//  state it gets back (see ../core/api.ts for the bridge).
// =========================================================================

import { LitElement, html } from 'lit';
import { customElement } from 'lit/decorators.js';
import type {
  ConsoleLevel,
  EditorConfig,
  MachineSnapshot,
  SubmitResult,
  TestVerdictDto,
} from '../core/types';
import * as api from '../core/api';
import * as gate from '../core/gate';
import { createEditor } from './editor';
import type { EditorHandle } from './editor';
import { createToolbar } from './toolbar';
import type { ToolbarCallbacks, ToolbarHandle } from './toolbar';
import { createDebugger } from './debugger';
import type { DebuggerHandle } from './debugger';
import { createConsole } from './console';
import type { ConsoleHandle } from './console';

@customElement('riscv-playground')
export class RiscvPlayground extends LitElement {
  config?: EditorConfig;
  initialCode = '';

  private _editor: EditorHandle | null = null;
  private _toolbar: ToolbarHandle | null = null;
  private _debugger: DebuggerHandle | null = null;
  private _console: ConsoleHandle | null = null;

  /** True while we are in debug mode (the toolbar switches faces). */
  private _debugging = false;
  /** Guards against two actions racing (double-clicked button, etc.). */
  private _busy = false;

  override createRenderRoot() {
    return this;
  }

  // ── Template ───────────────────────────────────────────────────────

  override render() {
    return html`
      <div class="riscv-playground" data-level=${this.config?.level ?? 'unknown'}>

        <!-- Zone 1: Editor -->
        <div class="riscv-editor">
          <div class="riscv-gutter"></div>
          <div class="riscv-lines"></div>
        </div>

        <!-- Zone 2: Controls -->
        <div class="riscv-controls">
          <div class="riscv-toolbar">
            <div class="riscv-toolbar-group riscv-toolbar--normal">
              <span class="riscv-toolbar-section">
                <button class="riscv-btn riscv-btn--primary" data-action="run">▶ 运行</button>
                <button class="riscv-btn" data-action="debug">🔍 调试</button>
              </span>
              <span class="riscv-toolbar-separator"></span>
              <span class="riscv-toolbar-section">
                <button class="riscv-btn riscv-btn--primary" data-action="submit">✓ 提交</button>
              </span>
              <span class="riscv-toolbar-separator"></span>
              <span class="riscv-menu-wrapper">
                <button class="riscv-btn riscv-btn--secondary" data-action="menu">☰</button>
                <div class="riscv-menu-dropdown" style="display:none">
                  <button class="riscv-menu-item" data-action="load">📂 加载文件</button>
                  <button class="riscv-menu-item" data-action="save">💾 保存文件</button>
                </div>
              </span>
            </div>

            <div class="riscv-toolbar-group riscv-toolbar--debug" style="display:none">
              <span class="riscv-toolbar-section">
                <button class="riscv-btn riscv-btn--primary" data-action="continue">▶ 继续</button>
                <span class="riscv-btn-pair">
                  <button class="riscv-btn" data-action="step">↓ 单步</button>
                  <button class="riscv-btn" data-action="step-back">↑ 倒退</button>
                </span>
              </span>
              <span class="riscv-toolbar-separator"></span>
              <span class="riscv-toolbar-section">
                <button class="riscv-btn riscv-btn--secondary" data-action="reset">↺ 重置</button>
                <button class="riscv-btn riscv-btn--secondary" data-action="quit-debug">✕ 退出</button>
              </span>
            </div>
          </div>

          <!-- Zone 2b: Machine state panel -->
          <div class="riscv-debug-panel" style="display:none">
            <div class="riscv-pc-summary"></div>
            <div class="riscv-reg-grid-container">
              <table class="riscv-reg-grid">
                ${this._renderRegGrid()}
              </table>
              <div class="riscv-reg-abi">ABI: sp=x2  ra=x1  gp=x3  tp=x4  fp=x8  a0=x10  a1=x11  a2=x12  a3=x13  a4=x14  a5=x15  a6=x16  a7=x17</div>
            </div>
            <div class="riscv-memory-watch">
              <div class="riscv-memory-addbar">
                <span class="riscv-memory-label">内存监视</span>
                <input class="riscv-memory-addr" type="text" placeholder="0x..." size="8">
                <button class="riscv-memory-add-btn">+ 添加</button>
              </div>
              <div class="riscv-memory-manual"></div>
              <div class="riscv-memory-auto"></div>
            </div>
          </div>
        </div>

        <!-- Zone 3: Console -->
        <div class="riscv-console">
          <div class="riscv-console-header">
            <span>控制台</span>
            <button class="riscv-console-clear" title="清除控制台">✕</button>
          </div>
          <div class="riscv-console-body">
            <div class="riscv-console-entry riscv-console-entry--empty">提交后在此查看评测结果</div>
          </div>
        </div>

      </div>
    `;
  }

  // Build the 8×4 register grid (static HTML; cells are filled by debugger.ts)
  private _renderRegGrid() {
    const rows = [];
    for (let r = 0; r < 8; r++) {
      const cells = [];
      for (let c = 0; c < 4; c++) {
        const idx = r + c * 8;
        cells.push(html`
          <td data-reg="${idx}" class="${idx === 0 ? 'riscv-reg--zero' : ''}">
            <span class="riscv-reg-name">x${idx}</span>
            <span class="riscv-reg-value">0x00000000</span>
          </td>
        `);
      }
      rows.push(html`<tr>${cells}</tr>`);
    }
    return rows;
  }

  // ── Lifecycle ──────────────────────────────────────────────────────

  protected override firstUpdated(): void {
    if (!this.config) return;

    this._editor = createEditor(this, this.config, this.initialCode);
    this._debugger = createDebugger(this);
    this._console = createConsole(this);

    const tbCfg = this.config.toolbar ?? {};
    const cb: ToolbarCallbacks = {
      onRun: () => this._run(),
      onDebug: () => this._debugStart(),
      onSubmit: () => this._submit(),
      onLoad: () => this._load(),
      onSave: () => this._save(),
      onContinue: () => this._debugCont(),
      onStep: () => this._debugStep(1),
      onStepBack: () => this._debugStep(-1),
      onReset: () => this._debugReset(),
      onQuitDebug: () => this._debugQuit(),
    };
    this._toolbar = createToolbar(this, tbCfg, cb);

    // Pull the authoritative level data from the backend. The markdown page is
    // only a fallback for browser preview (mdbook serve with no desktop shell).
    void this._loadFromBackend();
  }

  private async _loadFromBackend(): Promise<void> {
    const levelId = this.config?.level;
    if (!levelId) return;
    if (!api.isDesktop()) {
      this._log('info', '浏览器预览模式：编辑器可以写，但运行与判定要在桌面版里进行。');
      return;
    }
    try {
      const payload = await api.loadLevel(levelId);
      this._editor?.setCode(payload.initialCode);
      // 玩家可能从地址栏/历史记录直接走到还没解锁的关卡：不拦他，但要说明一句
      // （列表与判定都在 Rust 侧，见 core/gate.ts；这里只是复用它缓存的那份）
      if (gate.isUnlocked(levelId) === false) {
        const prev = gate.previousLevel(levelId);
        this._log('warning', gate.lockedNote(prev ? gate.levelTitle(prev.id) : '上一关'));
      }
      this._log('info', `关卡 ${payload.id}：${payload.title}`);
      if (payload.solved) this._log('success', '这一关你已经通过了 ✓');
      void api.saveProgress({ lastLevel: levelId });
    } catch (e) {
      this._log('error', `${(e as Error).message}`);
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────

  private _log(level: ConsoleLevel, msg: string): void {
    this._console?.append(level, msg);
  }

  private _begin(title: string): void {
    this._console?.clear();
    if (title) this._log('info', title);
    // 调试模式下"跑到一半的机器"和"重新跑一遍"是两回事，先说清楚
    if (this._debugging) {
      this._log('warning', '（当前处于调试模式：这次操作是重新跑一遍，不会影响调试会话）');
    }
  }

  private _getCode(): string {
    return this._editor?.collectCode() ?? '';
  }

  private _levelId(): string {
    return this.config?.level ?? 'unknown';
  }

  /** Runs an action with a single busy flag and uniform error reporting. */
  private async _guard(label: string, fn: () => Promise<void>): Promise<void> {
    if (this._busy) return;
    this._busy = true;
    try {
      await fn();
    } catch (e) {
      this._log('error', `${label}失败：${(e as Error).message}`);
    } finally {
      this._busy = false;
    }
  }

  // ── Run / Submit ───────────────────────────────────────────────────

  private _run(): void {
    return void this._guard('运行', async () => {
      this._begin('▶ 运行（不判定，只看它怎么跑）');
      const snap = await api.runCode(this._levelId(), this._getCode());
      this._renderSnapshot(snap, { showMachine: false });
    });
  }

  private _submit(): void {
    return void this._guard('提交', async () => {
      this._begin('✓ 提交判定中…');
      const result = await api.submitCode(this._levelId(), this._getCode());
      this._renderVerdict(result);
    });
  }

  /** Renders a run/step snapshot into the console (and optionally the panel). */
  private _renderSnapshot(snap: MachineSnapshot, opts: { showMachine: boolean }): void {
    if (snap.output) {
      this._log('info', `输出：${JSON.stringify(snap.output)}`);
    }
    this._log(snap.halted && snap.pauseKind !== 'error' ? 'success' : 'info', snap.pauseDetail);
    if (snap.exitCode !== undefined && snap.exitCode !== null) {
      this._log('info', `退出码：${snap.exitCode}`);
    }
    if (snap.pauseKind === 'error' || snap.pauseKind === 'limit') {
      this._log('error', snap.pauseDetail);
    }
    if (opts.showMachine) this._debugger?.setSnapshot(snap);
  }

  private _renderVerdict(v: SubmitResult): void {
    // 1) Compile/assemble errors
    if (v.errors.length > 0) {
      this._log('error', `没有编译通过（${v.errors.length} 个问题）`);
      for (const e of v.errors) {
        const where = e.line > 0 ? `第 ${e.line} 行：` : '';
        this._log('error', `  ${where}${e.message}`);
      }
      // The backend reports generated starlang assembly only when it compiled
      if (v.generatedAssembly) this._log('info', '生成的星之语：\n' + v.generatedAssembly);
      return;
    }

    // 2) Per-test verdicts
    if (v.passed) {
      this._log('success', `✓ ${v.summary}（得分 ${v.score}）`);
      if (v.firstClear) this._log('success', '★ 第一次通过这一关！');
    } else {
      this._log('error', `✗ ${v.summary}`);
    }

    for (const t of v.tests) {
      this._renderTest(t);
    }

    if (v.generatedAssembly && !v.passed) {
      this._log('info', '这段代码生成的星之语：\n' + v.generatedAssembly);
    }
  }

  private _renderTest(t: TestVerdictDto): void {
    this._log(t.passed ? 'success' : 'error', `${t.passed ? '✓' : '✗'} ${t.name}`);
    if (t.passed) {
      if (t.actualOutput) this._log('info', `   输出：${JSON.stringify(t.actualOutput)}`);
      return;
    }
    this._log('error', `   ${t.detail}`);
    for (const d of t.diffs) {
      this._log('info', `   ${d.what}：应为 ${d.expected}，实际 ${d.actual}`);
    }
    if (t.failure) {
      const f = t.failure;
      const where = f.line > 0 ? `第 ${f.line} 行` : '（行号未知）';
      this._log('info', `   失败现场：${where}，PC=0x${f.pc.toString(16).toUpperCase()}，${f.reasonText}`);
    }
  }

  // ── Debug actions ──────────────────────────────────────────────────

  private _debugStart(): void {
    return void this._guard('进入调试', async () => {
      this._begin('🔍 调试模式');
      const snap = await api.debugStart(this._levelId(), this._getCode());
      this._debugging = true;
      this._toolbar?.setMode('debug');
      this._debugger?.show();
      this._debugger?.setSnapshot(snap);
      this._log('info', '机器停在起点。用「单步」「倒退」「继续」观察寄存器与内存的变化。');
      if (snap.pauseDetail) this._log('info', snap.pauseDetail);
    });
  }

  private _debugCont(): void {
    return void this._guard('继续', async () => {
      const snap = await api.debugContinue();
      this._debugger?.setSnapshot(snap);
      this._renderSnapshot(snap, { showMachine: false });
    });
  }

  /** `dir` = 1 steps forward, -1 steps back. */
  private _debugStep(dir: number): void {
    return void this._guard(dir > 0 ? '单步' : '倒退', async () => {
      const r = dir > 0 ? await api.debugStep(1) : await api.debugStepBack(1);
      const { regChanges, memChanges, ...snap } = r;
      this._debugger?.renderStep(snap, regChanges, memChanges);

      const changed = Object.keys(regChanges ?? {});
      if (changed.length > 0) {
        const text = changed.map((k) => `${k}=0x${(regChanges[k] >>> 0).toString(16).toUpperCase()}`).join('  ');
        this._log('info', `第 ${snap.instructions} 条：${snap.pauseDetail}  ${text}`);
      }
      if (snap.pauseKind === 'error' || snap.pauseKind === 'limit') {
        this._log('error', snap.pauseDetail);
      }
    });
  }

  private _debugReset(): void {
    return void this._guard('重置', async () => {
      const snap = await api.debugReset();
      this._debugger?.setSnapshot(snap);
      this._log('info', '↺ 机器已回到起点。');
    });
  }

  private _debugQuit(): void {
    void api.debugQuit().catch(() => undefined);
    this._debugging = false;
    this._debugger?.hide();
    this._debugger?.reset();
    this._toolbar?.setMode('normal');
    this._log('info', '✕ 退出调试模式。');
  }

  // ── Load / Save (local files) ──────────────────────────────────────

  private _load(): void {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.s,.asm,.c,.txt';
    input.addEventListener('change', () => {
      const f = input.files?.[0];
      if (!f) return;
      const reader = new FileReader();
      reader.onload = () => {
        this._editor?.setCode(String(reader.result ?? ''));
        this._log('info', `已加载文件：${f.name}`);
      };
      reader.readAsText(f);
    });
    input.click();
  }

  private _save(): void {
    const code = this._getCode();
    const levelId = this._levelId();
    const ext = this.config?.language === 'c' ? 'c' : 's';
    const name = `level-${levelId}.${ext}`;
    const blob = new Blob([code], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    this._log('info', `文件已下载：${name}`);
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'riscv-playground': RiscvPlayground;
  }
}
