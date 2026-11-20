import { useState } from 'react';
import { ChevronDown, ChevronRight, Lightbulb } from 'lucide-react';
import type { HintItem } from '../../engine/types';

interface HintDrawerProps {
  hints: HintItem[];
}

const LEVEL_LABELS: Record<number, string> = {
  1: '提示 1 · 思路方向',
  2: '提示 2 · 步骤拆解',
  3: '提示 3 · 关键代码片段',
};

export function HintDrawer({ hints }: HintDrawerProps) {
  const [revealed, setRevealed] = useState(0);

  if (hints.length === 0) return null;

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900/70 p-3">
      <div className="flex items-center justify-between">
        <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-amber-300">
          <Lightbulb className="h-3.5 w-3.5" /> 3 级按需提示（已展开 {revealed}/{hints.length}）
        </span>
        {revealed < hints.length ? (
          <button
            type="button"
            onClick={() => setRevealed((r) => Math.min(hints.length, r + 1))}
            className="inline-flex items-center gap-1 rounded-md border border-amber-500/40 bg-amber-500/15 px-2.5 py-1 text-[11px] font-semibold text-amber-200 hover:bg-amber-500/25"
          >
            <ChevronDown className="h-3 w-3" /> 查看下一级提示
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setRevealed(0)}
            className="inline-flex items-center gap-1 rounded-md border border-slate-700 bg-slate-800 px-2 py-1 text-[11px] text-slate-400 hover:text-slate-200"
          >
            <ChevronRight className="h-3 w-3" /> 收起全部
          </button>
        )}
      </div>

      {revealed > 0 && (
        <div className="mt-2.5 space-y-2">
          {hints.slice(0, revealed).map((hint) => (
            <div
              key={hint.level}
              className="rounded-lg border border-amber-500/25 bg-amber-950/20 p-2.5 text-xs"
            >
              <div className="mb-1 font-semibold text-amber-300">
                {LEVEL_LABELS[hint.level] ?? `提示 ${hint.level}`}
              </div>
              {hint.type === 'snippet' ? (
                <pre className="overflow-x-auto rounded bg-slate-950 p-2 font-mono text-xs text-cyan-200">
                  {hint.content}
                </pre>
              ) : (
                <p className="leading-relaxed text-slate-200">{hint.content}</p>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
