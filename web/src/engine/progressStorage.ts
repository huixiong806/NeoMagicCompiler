import type { SubmitDto, UserProgress } from './types';

const PROGRESS_KEY = 'nmc.progress.v1';
const DRAFT_PREFIX = 'nmc.draft.v1.';

export function defaultProgress(): UserProgress {
  return {
    version: 1,
    lastModified: new Date().toISOString(),
    levels: {},
    settings: {
      showHints: true,
      freeExplore: false,
    },
  };
}

export function loadProgress(): UserProgress {
  try {
    const raw = window.localStorage.getItem(PROGRESS_KEY);
    if (!raw) return defaultProgress();
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return defaultProgress();
    const candidate = parsed as Partial<UserProgress>;
    return {
      version: typeof candidate.version === 'number' ? candidate.version : 1,
      lastModified:
        typeof candidate.lastModified === 'string'
          ? candidate.lastModified
          : new Date().toISOString(),
      levels:
        candidate.levels && typeof candidate.levels === 'object' ? candidate.levels : {},
      settings: {
        showHints: candidate.settings?.showHints ?? true,
        freeExplore: candidate.settings?.freeExplore ?? false,
      },
    };
  } catch {
    return defaultProgress();
  }
}

export function saveProgress(progress: UserProgress): UserProgress {
  const next: UserProgress = {
    ...progress,
    lastModified: new Date().toISOString(),
  };
  try {
    window.localStorage.setItem(PROGRESS_KEY, JSON.stringify(next));
  } catch {
    // 无痕模式或配额满时不中断游戏
  }
  return next;
}

export function recordSubmissionProgress(submit: SubmitDto): UserProgress {
  const current = loadProgress();
  const prev = current.levels[submit.level];
  const now = new Date().toISOString();
  const passed = submit.passed || Boolean(prev?.passed);
  const bestScore =
    prev?.bestScore !== undefined ? Math.max(prev.bestScore, submit.score) : submit.score;

  current.levels[submit.level] = {
    levelId: submit.level,
    passed,
    completedAt: prev?.completedAt ?? (submit.passed ? now : undefined),
    attempts: (prev?.attempts ?? 0) + 1,
    bestScore,
    metPar: Boolean(prev?.metPar || submit.metPar),
  };
  return saveProgress(current);
}

export function updateSettings(patch: Partial<UserProgress['settings']>): UserProgress {
  const current = loadProgress();
  current.settings = { ...current.settings, ...patch };
  return saveProgress(current);
}

export function loadDraft(key: string): string | null {
  try {
    return window.localStorage.getItem(`${DRAFT_PREFIX}${key}`);
  } catch {
    return null;
  }
}

export function saveDraft(key: string, code: string): void {
  try {
    window.localStorage.setItem(`${DRAFT_PREFIX}${key}`, code);
  } catch {
    // 忽略存储配额错误
  }
}

export function clearDraft(key: string): void {
  try {
    window.localStorage.removeItem(`${DRAFT_PREFIX}${key}`);
  } catch {
    // 忽略
  }
}
