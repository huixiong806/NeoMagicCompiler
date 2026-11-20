import { ArrowDown, CornerDownRight, Play, RotateCcw } from 'lucide-react';
import type { ManualCpuSandboxConfig, PcTrackInstruction } from '../../engine/types';

interface PcTrackDeskProps {
  config: ManualCpuSandboxConfig;
  caseIndex: number;
  pcIndex: number;
  halted: boolean;
  regs: Record<string, number>;
  onSwitchCase: (index: number) => void;
  onStep: (choice?: 'taken' | 'not-taken') => void;
}

function isBranchOp(op: PcTrackInstruction['op']): boolean {
  return op === 'blt' || op === 'bge' || op === 'beq' || op === 'bne';
}

function branchSymbol(op: PcTrackInstruction['op']): string {
  if (op === 'blt') return '<';
  if (op === 'bge') return '≥';
  if (op === 'beq') return '=';
  return '≠';
}

export function PcTrackDesk({
  config,
  caseIndex,
  pcIndex,
  halted,
  regs,
  onSwitchCase,
  onStep,
}: PcTrackDeskProps) {
  const track = config.pcTrack;
  if (!track) return null;

  const current = halted ? undefined : track.program[pcIndex];
  const cases = track.cases ?? [];

  return (
    <div className="space-y-2.5 rounded-2xl border border-slate-800 bg-slate-950/90 p-3">
      {cases.length > 1 && (
        <div className="flex flex-wrap gap-1.5">
          {cases.map((c, idx) => (
            <button
              key={c.label}
              type="button"
              onClick={() => onSwitchCase(idx)}
              className={`rounded-full px-2.5 py-0.5 font-mono text-[11px] transition ${
                idx === caseIndex
                  ? 'bg-violet-500 font-bold text-slate-950'
                  : 'bg-slate-900 text-slate-400 hover:bg-slate-800 hover:text-slate-200'
              }`}
            >
              {c.label}
            </button>
          ))}
        </div>
      )}

      {/* 指令轨道可视化 */}
      <div className="space-y-1">
        {track.program.map((inst, idx) => {
          const active = !halted && idx === pcIndex;
          const targetIdx = inst.target
            ? track.program.findIndex((p) => p.label === inst.target)
            : -1;
          const isBackward = targetIdx >= 0 && targetIdx <= idx;

          return (
            <div
              key={`${idx}-${inst.asm}`}
              className={`flex items-center justify-between rounded-lg border px-2.5 py-1.5 font-mono text-xs transition ${
                active
                  ? 'border-emerald-400 bg-emerald-500/15 text-white shadow-[0_0_12px_rgba(16,185,129,0.2)]'
                  : 'border-slate-800/80 bg-slate-900/60 text-slate-400'
              }`}
            >
              <div className="flex items-center gap-2">
                <span
                  className={`inline-flex w-9 justify-center rounded px-1 py-0.5 text-[10px] font-bold ${
                    active ? 'bg-emerald-400 text-slate-950' : 'bg-slate-800 text-slate-500'
                  }`}
                >
                  {active ? 'PC▶' : idx + 1}
                </span>
                {inst.label && (
                  <span className="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-bold text-violet-300">
                    {inst.label}:
                  </span>
                )}
                <span className={active ? 'font-bold text-emerald-100' : ''}>{inst.asm}</span>
              </div>

              {inst.target && (
                <span
                  className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold ${
                    isBackward
                      ? 'bg-amber-500/20 text-amber-300'
                      : 'bg-sky-500/15 text-sky-300'
                  }`}
                >
                  {isBackward ? (
                    <RotateCcw className="h-3 w-3" />
                  ) : (
                    <CornerDownRight className="h-3 w-3" />
                  )}
                  {inst.target}
                </span>
              )}
            </div>
          );
        })}
      </div>

      {/* 极简比较器 / 单步控制台 */}
      {current && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-emerald-500/35 bg-emerald-950/20 px-3 py-2">
          {isBranchOp(current.op) ? (
            <>
              <div className="flex items-center gap-1.5 font-mono text-xs font-bold">
                <span className="rounded bg-slate-900 px-2 py-0.5 text-cyan-300">
                  {current.a}={regs[current.a ?? ''] ?? 0}
                </span>
                <span className="text-violet-300">{branchSymbol(current.op)}</span>
                <span className="rounded bg-slate-900 px-2 py-0.5 text-cyan-300">
                  {current.b}={regs[current.b ?? ''] ?? 0}
                </span>
                <span className="text-slate-400">?</span>
              </div>
              <div className="flex items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => onStep('taken')}
                  className="inline-flex items-center gap-1 rounded-lg bg-emerald-400 px-2.5 py-1 font-mono text-xs font-bold text-slate-950 hover:bg-emerald-300"
                >
                  ✓ → {current.target}
                </button>
                <button
                  type="button"
                  onClick={() => onStep('not-taken')}
                  className="inline-flex items-center gap-1 rounded-lg border border-slate-600 bg-slate-800 px-2.5 py-1 text-xs font-semibold text-slate-200 hover:bg-slate-700"
                >
                  <ArrowDown className="h-3 w-3" /> 顺序
                </button>
              </div>
            </>
          ) : (
            <>
              <span className="font-mono text-xs font-semibold text-emerald-200">{current.asm}</span>
              <button
                type="button"
                onClick={() => onStep()}
                className="inline-flex items-center gap-1 rounded-lg bg-emerald-400 px-3 py-1 text-xs font-bold text-slate-950 hover:bg-emerald-300"
              >
                <Play className="h-3 w-3" />
                {current.op === 'j' ? `→ ${current.target}` : current.op === 'halt' ? '结束' : '走一步'}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}
