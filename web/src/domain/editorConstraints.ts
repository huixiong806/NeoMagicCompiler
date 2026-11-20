/**
 * 编辑器行级只读前后缀保护的纯计算逻辑（供 CodeMirror 6 事务过滤器与单测共享）。
 */

export interface ReadonlyBounds {
  /** 前 N 行只读（1-based 行号 `1..=readonlyPrefix`） */
  readonlyPrefix: number;
  /** 后 N 行只读（最后 `readonlySuffix` 行） */
  readonlySuffix: number;
}

/**
 * 判断一次对 `lineCount` 行文档的区间修改 `[fromLine, toLine]`（1-based 闭区间）是否越界触碰了只读前后缀。
 */
export function touchesReadonlyLines(
  lineCount: number,
  fromLine: number,
  toLine: number,
  bounds: ReadonlyBounds,
): boolean {
  if (lineCount <= 0) return false;
  const prefix = Math.max(0, bounds.readonlyPrefix);
  const suffix = Math.max(0, bounds.readonlySuffix);
  if (prefix === 0 && suffix === 0) return false;

  const firstEditable = prefix + 1;
  const lastEditable = lineCount - suffix;
  if (firstEditable > lastEditable) return true;

  return fromLine < firstEditable || toLine > lastEditable;
}

/**
 * 计算当前文档中所有只读行号集合（1-based）。
 */
export function getReadonlyLineSet(lineCount: number, bounds: ReadonlyBounds): Set<number> {
  const set = new Set<number>();
  for (let i = 1; i <= Math.min(bounds.readonlyPrefix, lineCount); i++) {
    set.add(i);
  }
  for (let i = 0; i < Math.min(bounds.readonlySuffix, lineCount); i++) {
    set.add(lineCount - i);
  }
  return set;
}
