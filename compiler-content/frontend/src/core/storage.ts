// =========================================================================
//  StorageManager — localStorage auto-save / restore for code drafts
// =========================================================================

import type { Draft } from './types';
import { storageKey } from './parser';

const AUTO_SAVE_DEBOUNCE_MS = 500;

/**
 * Try to restore a saved draft for the given level.
 * Returns the code string, or null if nothing was saved.
 */
export function restoreDraft(levelId: string): string | null {
  const key = storageKey(levelId);
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const draft: Draft = JSON.parse(raw);
    return draft && typeof draft.code === 'string' ? draft.code : null;
  } catch {
    try {
      localStorage.removeItem(key);
    } catch {
      /* ignore */
    }
    return null;
  }
}

/**
 * Persist the current code as a draft.
 */
export function saveDraft(levelId: string, code: string): void {
  const key = storageKey(levelId);
  try {
    const draft: Draft = {
      code,
      savedAt: new Date().toISOString(),
      level: levelId,
    };
    localStorage.setItem(key, JSON.stringify(draft));
  } catch (e) {
    console.warn('[riscv-playground] 自动存档失败:', (e as Error).message);
  }
}

/**
 * Create a debounced auto-save scheduler.
 * Returns { schedule, flush }.
 */
export function createAutoSave(levelId: string, getCode: () => string) {
  let timer: ReturnType<typeof setTimeout> | null = null;

  function flush(): void {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    saveDraft(levelId, getCode());
  }

  function schedule(): void {
    if (timer) clearTimeout(timer);
    timer = setTimeout(flush, AUTO_SAVE_DEBOUNCE_MS);
  }

  return { schedule, flush };
}
