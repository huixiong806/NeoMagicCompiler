// =========================================================================
//  Progress gate — "通过才解锁"（只影响**侧边栏**，不拦玩家）
//
//  这一层只做一件事：把 mdBook 侧边栏里**还没解锁**的关卡标成"还没打开"，
//  让它点不动，并给一句 tooltip 解释为什么。故意的玩家从地址栏进来我们不管 ——
//  单机游戏不跟玩家较劲，这里的目标只是"别让人不小心走错路"。
//
//  规则本身不在前端：哪一关是解锁的由 `list_levels` 的 `unlocked` 字段说了算
//  （判定在 Rust 领域层 `UserProgress::is_unlocked`，有单元测试）。
//  前端**不要**自己按 `passed` 推：解锁是"按关卡顺序"算的，只看一项推不出来。
//
//  代价：每页一次 `list_levels`。这一次 IPC 同时供侧边栏与关卡页里的
//  "你其实还没解锁这一关"提示使用，所以还是只有一次。
// =========================================================================

import * as api from './api';
import type { LevelSummary } from './types';

/** 未解锁的侧边栏项加的 class（样式见本文件末尾注入的 `SIDEBAR_STYLE`）。 */
export const LOCKED_CLASS = 'nmc-toc-locked';

/** 未解锁时的 tooltip：要一眼看懂是"还没解锁"，而不是"页面坏了"。 */
export const LOCKED_HINT = '还没解锁：先把上一关通了';

/** 故意走到未解锁的关卡时，控制台里那句世界观说明。`name` 是上一关的标题。 */
export function lockedNote(name: string): string {
  return `星泠：这一层我们其实还没弄明白——${name}还没通。不过你想先看看也行。`;
}

/** 上一关的显示名兜底（拿不到侧边栏标题时用）。 */
const PREVIOUS = '上一关';

// ── 数据 ───────────────────────────────────────────────────────────────

/** 一次 IPC 的结果缓存：一次都没问过 / 问失败了 / 拿到了。 */
let levels: LevelSummary[] | null | undefined;
let inflight: Promise<LevelSummary[] | null | undefined> | null = null;

/** 关卡 ID → 页面文件名（mdBook 的 `cN_M.html`；保持前导零，别归一化）。 */
export function levelPage(levelId: string): string {
  const [chapter, number] = levelId.split('-');
  return `c${chapter}_${number}.html`;
}

/** 问一次后端并缓存。桌面壳不在（mdbook serve 预览）时什么都不做，也不重试。 */
export async function ensureLevels(): Promise<LevelSummary[] | null | undefined> {
  if (levels) return levels;
  if (inflight) return inflight;
  if (!api.isDesktop()) return undefined;
  inflight = (async () => {
    try {
      levels = await api.listLevels();
    } catch {
      // 门禁失败不该影响游戏本身：没拿到列表就什么都不标
      levels = null;
    } finally {
      inflight = null;
    }
    return levels;
  })();
  return inflight;
}

/**
 * 这一关现在解锁了吗？
 *
 * - `true` / `false`：后端说了算（见文件头的说明）；
 * - `null`：还不知道（后端拿不到列表，比如浏览器预览）；
 * - `undefined`：列表里没有这一关（正常不该发生，按"不提示"处理）。
 */
export function isUnlocked(levelId: string): boolean | null | undefined {
  if (!levels) return null;
  return levels.find((l) => l.id === levelId)?.unlocked;
}

/** 关卡 ID → 标题（用来在提示里报出上一关的名字）。 */
export function levelTitle(levelId: string): string {
  return levels?.find((l) => l.id === levelId)?.title ?? PREVIOUS;
}

/**
 * 关卡 ID 的**上一关**（按后端给的顺序，所以跨章也对：`2-1` 的上一关是 `1-15`），
 * 且这一关的解锁与否取决于它。列表里没有这一关时返回 `null`。
 */
export function previousLevel(levelId: string) {
  const i = levels?.findIndex((l) => l.id === levelId) ?? -1;
  return i > 0 ? levels![i - 1] : null;
}

// ── 侧边栏 ─────────────────────────────────────────────────────────────

/**
 * 只改**外观与可点性**，不动 mdBook 的目录结构（不改 `SUMMARY.md`、不重建 HTML）：
 * 给未解锁项加 `LOCKED_CLASS` + `title` + `aria-disabled`，并去掉 `href`
 * （去掉 `href` 之后连"右键→在新标签页打开"也点不动了，这是刻意的）。
 *
 * 侧边栏在 mdBook 0.5 里是 `<iframe src="toc.html">`，所以链接不在本文档里 ——
 * 那些 iframe 也要一起处理。
 */
export function markSidebar(): void {
  if (!levels) return;
  const lockedPages = new Set(
    levels.filter((l) => !l.unlocked).map((l) => levelPage(l.id)),
  );
  // 本文档的新版布局 + 内联旧版布局 + 侧边栏 iframe（0.5 的默认布局）
  const docs: Document[] = [document];
  document.querySelectorAll('iframe').forEach((f) => {
    // 跨文档读 contentDocument 可能抛异常（不同源时），忽略即可
    try {
      if (f.contentDocument) docs.push(f.contentDocument);
    } catch {
      /* 读不到就算了：那个 iframe 里没有目录 */
    }
  });

  for (const doc of docs) {
    doc.querySelectorAll<HTMLAnchorElement>('a[href]').forEach((a) => {
      const file = a.getAttribute('href')?.split(/[?#]/)[0] ?? '';
      if (!lockedPages.has(file)) return;
      if (a.classList.contains(LOCKED_CLASS)) return;
      a.classList.add(LOCKED_CLASS);
      a.title = LOCKED_HINT;
      a.setAttribute('aria-disabled', 'true');
      a.removeAttribute('href');
    });
  }
}

/**
 * 侧边栏那条简单的样式：复用编辑器里"只读行"的视觉语言（`editor.css` 的
 * `.riscv-line--readonly` 用同一套变量：`var(--icons)` + 降透明度 + `not-allowed`），
 * 这样它看起来和别处的"锁住"是同一种东西，而不是新造一套。
 */
const SIDEBAR_STYLE = `
.${LOCKED_CLASS} {
  color: var(--icons);
  opacity: 0.6;
  cursor: not-allowed;
}
html[data-theme="dark"] .${LOCKED_CLASS},
body.dark .${LOCKED_CLASS} {
  opacity: 0.5;
}
`;

/** 注入上面那份样式（每页一次；只做一次，重复调用无害）。 */
let styled = false;
export function injectGateStyle(): void {
  if (styled) return;
  styled = true;
  const style = document.createElement('style');
  style.dataset.nmcGate = 'true';
  style.textContent = SIDEBAR_STYLE;
  document.head.appendChild(style);
}
