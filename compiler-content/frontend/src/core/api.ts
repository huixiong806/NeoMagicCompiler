// =========================================================================
//  Backend bridge — the single place that talks to Tauri
//
//  Everything else in the frontend calls these functions instead of poking
//  `window.__TAURI__` directly, so that:
//    * when the page is opened in a plain browser (`mdbook serve`, no Tauri)
//      every call fails with one clear error instead of `undefined is not a
//      function` deep inside a component;
//    * the command names and argument shapes live in exactly one file.
//
//  Argument names MUST stay camelCase: Tauri converts them to the snake_case
//  Rust parameters.
// =========================================================================

import type {
  LevelPayload,
  LevelSummary,
  MachineSnapshot,
  StepResult,
  SubmitResult,
} from './types';

/** Minimal shape of the Tauri global (`withGlobalTauri: true`). */
interface TauriGlobal {
  core?: { invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> };
}

/** Is the page running inside the desktop shell? */
export function isDesktop(): boolean {
  const g = (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__;
  return !!g?.core?.invoke;
}

/** One clear error message for "you opened this in a browser". */
export class NotDesktopError extends Error {
  constructor() {
    super(
      '这是浏览器预览模式，还没有连上魔导核心。请用 `make dev` 启动桌面版，' +
        '或在 MDV 里查看叙事内容。',
    );
    this.name = 'NotDesktopError';
  }
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const g = (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__;
  if (!g?.core?.invoke) throw new NotDesktopError();
  try {
    return await g.core.invoke<T>(cmd, args);
  } catch (e) {
    // Rust 端返回的错误是一句话字符串，这里保证拿到的永远是 Error
    throw new Error(typeof e === 'string' ? e : String((e as Error)?.message ?? e));
  }
}

// ── Levels & progress ──────────────────────────────────────────────────

export function listLevels(): Promise<LevelSummary[]> {
  return invoke<LevelSummary[]>('list_levels');
}

export function loadLevel(level: string): Promise<LevelPayload> {
  return invoke<LevelPayload>('load_level', { level });
}

export function loadProgress(): Promise<unknown> {
  return invoke<unknown>('load_progress');
}

export function saveProgress(patch?: {
  showHints?: boolean;
  lastLevel?: string;
}): Promise<unknown> {
  return invoke<unknown>('save_progress', { patch: patch ?? {} });
}

/** About / diagnostics: version, level count, where the content came from. */
export function appInfo(): Promise<{
  version: string;
  levels: number;
  contentRoot: string;
  progressFile: string;
  embeddedFiles: number;
  stackTop: number;
  bootAddr: number;
}> {
  return invoke('app_info');
}

// ── Run ────────────────────────────────────────────────────────────────

export function runCode(level: string, code: string): Promise<MachineSnapshot> {
  return invoke<MachineSnapshot>('run_code', { level, code });
}

// ── Debug ──────────────────────────────────────────────────────────────

export function debugStart(level: string, code: string): Promise<MachineSnapshot> {
  return invoke<MachineSnapshot>('debug_start', { level, code });
}

export function debugStep(n = 1): Promise<StepResult> {
  return invoke<StepResult>('debug_step', { n });
}

export function debugStepBack(n = 1): Promise<StepResult> {
  return invoke<StepResult>('debug_step_back', { n });
}

export function debugContinue(): Promise<MachineSnapshot> {
  return invoke<MachineSnapshot>('debug_continue');
}

export function debugReset(): Promise<MachineSnapshot> {
  return invoke<MachineSnapshot>('debug_reset');
}

export function debugQuit(): Promise<void> {
  return invoke<void>('debug_quit');
}

export function debugState(): Promise<MachineSnapshot | null> {
  return invoke<MachineSnapshot | null>('debug_state');
}

// ── Submit ─────────────────────────────────────────────────────────────

export function submitCode(level: string, code: string): Promise<SubmitResult> {
  return invoke<SubmitResult>('submit_code', { level, code });
}
