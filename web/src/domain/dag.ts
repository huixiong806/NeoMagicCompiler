import type { LevelSummary } from '../engine/types';

export interface DagSpineNode {
  main: LevelSummary;
  sideChallenges: LevelSummary[];
}

/**
 * 将单章关卡整理为主线主干（`lesson` / `practice`）+ 挂接在对应父节点旁的支线挑战（`challenge`）。
 * 保持纯函数与数据驱动，适配任意章节 DAG 拓扑。
 */
export function buildChapterSpine(levels: readonly LevelSummary[]): DagSpineNode[] {
  const sorted = [...levels].sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
  const mainNodes = sorted.filter((l) => l.kind !== 'challenge');
  const challenges = sorted.filter((l) => l.kind === 'challenge');

  const spine: DagSpineNode[] = mainNodes.map((main) => ({
    main,
    sideChallenges: [],
  }));

  const byId = new Map<string, DagSpineNode>();
  for (const node of spine) {
    byId.set(node.main.id, node);
  }

  for (const ch of challenges) {
    const parentId = ch.prerequisites[0];
    const parent = parentId ? byId.get(parentId) : undefined;
    if (parent) {
      parent.sideChallenges.push(ch);
    } else if (spine.length > 0) {
      spine[spine.length - 1]!.sideChallenges.push(ch);
    } else {
      spine.push({ main: ch, sideChallenges: [] });
    }
  }

  return spine;
}

/** 推荐玩家下一关要打的主线关（若主线全通，再推荐未通支线）。 */
export function nextRecommendedLevel(levels: readonly LevelSummary[]): LevelSummary | undefined {
  const mainNext = levels.find((l) => l.kind !== 'challenge' && l.unlocked && !l.passed);
  if (mainNext) return mainNext;
  return levels.find((l) => l.unlocked && !l.passed);
}
