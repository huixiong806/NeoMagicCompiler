import { useEffect, useState } from 'react';
import { ArrowRight, Check, RotateCcw, Target, Terminal, Undo2 } from 'lucide-react';
import { getRegisterInfo } from '../../domain/registers';
import type { ManualCpuAluOp, ManualCpuSandboxConfig } from '../../engine/types';
import { AluDesk } from './AluDesk';
import { PcTrackDesk } from './PcTrackDesk';
import {
  applyAluOperation,
  applyDirectCopy,
  createManualCpuState,
  undoManualCpu,
  type ManualCpuState,
  type SourceOperand,
} from './model';
import { applyPcTrackStep } from './pcTrackModel';

export interface ManualCpuSandboxProps {
  config: ManualCpuSandboxConfig;
  onFocusEditor?: () => void;
}

export function ManualCpuSandbox({ config, onFocusEditor }: ManualCpuSandboxProps) {
  const [state, setState] = useState<ManualCpuState>(() => createManualCpuState(config, 0));
  const [left, setLeft] = useState<SourceOperand | null>(null);
  const [right, setRight] = useState<SourceOperand | null>(null);
  const [copySrc, setCopySrc] = useState<SourceOperand | null>(null);
  const [activeSlot, setActiveSlot] = useState<'left' | 'right' | 'copySrc'>('left');
  const [op, setOp] = useState<ManualCpuAluOp>(config.aluOps?.[0] ?? '+');
  const [destReg, setDestReg] = useState<string>(
    () =>
      Object.keys(config.warmupPuzzle.goal.regs ?? {}).find((r) => r !== 'zero') ??
      config.focusRegs.find((r) => r !== 'zero') ??
      't2',
  );
  const [flashNonce, setFlashNonce] = useState(0);

  useEffect(() => {
    setState(createManualCpuState(config, 0));
    setLeft(null);
    setRight(null);
    setCopySrc(null);
    setActiveSlot('left');
    setOp(config.aluOps?.[0] ?? '+');
    const defaultDest =
      Object.keys(config.warmupPuzzle.goal.regs ?? {}).find((r) => r !== 'zero') ??
      config.focusRegs.find((r) => r !== 'zero') ??
      't2';
    setDestReg(defaultDest);
  }, [config]);

  const hasAlu = config.modules.includes('alu');
  const hasConstants = config.modules.includes('constants') && (config.constantCards?.length ?? 0) > 0;
  const hasPcTrack = config.modules.includes('pcTrack');

  const handleSelectOperand = (operand: SourceOperand) => {
    if (!hasAlu && !config.allowCopy) return;
    if (activeSlot === 'copySrc') {
      setCopySrc(operand);
      return;
    }
    if (activeSlot === 'left') {
      setLeft(operand);
      setActiveSlot('right');
    } else {
      setRight(operand);
      setActiveSlot('left');
    }
  };

  const handleDragStart = (operand: SourceOperand) => (e: React.DragEvent) => {
    e.dataTransfer.setData('application/x-nmc-operand', JSON.stringify(operand));
    e.dataTransfer.effectAllowed = 'copy';
  };

  const handleDropOnReg = (targetReg: string) => (e: React.DragEvent) => {
    if (!config.allowCopy) return;
    e.preventDefault();
    try {
      const parsed = JSON.parse(
        e.dataTransfer.getData('application/x-nmc-operand'),
      ) as SourceOperand;
      if (parsed && (parsed.kind === 'reg' || parsed.kind === 'const')) {
        setState((prev) => applyDirectCopy(prev, config, parsed, targetReg));
        setFlashNonce((n) => n + 1);
      }
    } catch {
      // 忽略无效拖拽数据
    }
  };

  const handleReset = (nextCaseIndex = state.caseIndex) => {
    setState(createManualCpuState(config, nextCaseIndex));
    setLeft(null);
    setRight(null);
    setCopySrc(null);
    setActiveSlot('left');
  };

  const activeGoalRegs =
    config.pcTrack?.cases?.[state.caseIndex]?.goal.regs ?? config.warmupPuzzle.goal.regs ?? {};

  return (
    <div className="space-y-3 rounded-2xl border border-slate-800 bg-slate-900/75 p-3.5">
      {/* 极简图形目标头 + 图标控制 */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-1.5">
          <Target className="h-3.5 w-3.5 text-cyan-400" />
          {Object.entries(activeGoalRegs).map(([reg, want]) => {
            const hit = (state.regs[reg] ?? 0) === want;
            return (
              <span
                key={reg}
                className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 font-mono text-xs font-bold transition ${
                  hit
                    ? 'border-emerald-400/70 bg-emerald-500/20 text-emerald-200'
                    : 'border-slate-700 bg-slate-950 text-slate-300'
                }`}
              >
                {reg} = {want}
                {hit && <Check className="h-3 w-3 text-emerald-300" />}
              </span>
            );
          })}
        </div>

        <div className="flex items-center gap-1">
          <button
            type="button"
            title="撤销一步"
            disabled={state.undoStack.length === 0}
            onClick={() => setState((prev) => undoManualCpu(prev))}
            className="rounded-lg border border-slate-800 bg-slate-950 p-1.5 text-slate-400 hover:bg-slate-800 hover:text-white disabled:opacity-35"
          >
            <Undo2 className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            title="重置沙盘"
            onClick={() => handleReset(state.caseIndex)}
            className="rounded-lg border border-slate-800 bg-slate-950 p-1.5 text-slate-400 hover:bg-slate-800 hover:text-white"
          >
            <RotateCcw className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* 寄存器芯片卡片阵 */}
      <div className="grid grid-cols-3 gap-2">
        {config.focusRegs.map((regName) => {
          const val = state.regs[regName] ?? 0;
          const info = getRegisterInfo(regName);
          const goalVal = activeGoalRegs[regName];
          const goalHit = goalVal !== undefined && val === goalVal;
          const flashed = state.lastWrittenReg === regName;
          const isWarnFlash = flashed && state.feedback?.tone === 'warn';

          return (
            <div
              key={`${regName}-${flashed ? flashNonce : 0}`}
              draggable={hasAlu || Boolean(config.allowCopy)}
              onDragStart={handleDragStart({ kind: 'reg', name: regName })}
              onDragOver={(e) => {
                if (config.allowCopy) e.preventDefault();
              }}
              onDrop={handleDropOnReg(regName)}
              onClick={() => handleSelectOperand({ kind: 'reg', name: regName })}
              className={`cursor-pointer select-none rounded-xl border px-3 py-2 text-center transition ${
                isWarnFlash ? 'nmc-flash-warn' : flashed ? 'nmc-flash-write' : ''
              } ${
                goalHit
                  ? 'border-emerald-400/80 bg-emerald-950/35 shadow-[0_0_12px_rgba(16,185,129,0.18)]'
                  : 'border-slate-700/90 bg-slate-950/90 hover:border-cyan-400/70'
              }`}
            >
              <div className="flex items-center justify-between font-mono text-[10px]">
                <span className="font-bold text-amber-300">{regName}</span>
                <span className="text-slate-600">{info?.xName ?? ''}</span>
              </div>
              <div className="mt-1 font-mono text-2xl font-bold tracking-tight text-cyan-200">
                {val}
              </div>
              {goalVal !== undefined && (
                <div
                  className={`mt-0.5 font-mono text-[10px] ${
                    goalHit ? 'text-emerald-400' : 'text-slate-500'
                  }`}
                >
                  {goalHit ? '✓' : `→ ${goalVal}`}
                </div>
              )}
            </div>
          );
        })}

        {/* 常量卡片直接并入芯片阵 */}
        {hasConstants &&
          config.constantCards!.map((card) => (
            <button
              key={`${card.kind}-${card.label}`}
              type="button"
              draggable
              onDragStart={handleDragStart({ kind: 'const', card })}
              onClick={() => handleSelectOperand({ kind: 'const', card })}
              className="flex flex-col items-center justify-center rounded-xl border border-amber-500/50 bg-amber-500/10 px-3 py-2 font-mono transition hover:border-amber-400 hover:bg-amber-500/20"
            >
              <span className="text-[10px] font-bold text-amber-400">IMM</span>
              <span className="mt-1 text-2xl font-bold text-amber-200">#{card.label}</span>
            </button>
          ))}
      </div>

      {/* ALU 电路芯片台 */}
      {hasAlu && (
        <AluDesk
          config={config}
          regs={state.regs}
          left={left}
          right={right}
          op={op}
          destReg={destReg}
          activeSlot={activeSlot}
          copySrc={copySrc}
          onSelectSlot={setActiveSlot}
          onSelectOp={setOp}
          onSelectDest={setDestReg}
          onDropToSlot={(slot, payload) => {
            if (slot === 'left') setLeft(payload);
            else setRight(payload);
          }}
          onExecuteAlu={() => {
            if (!left || !right) return;
            setState((prev) => applyAluOperation(prev, config, left, op, right, destReg));
            setFlashNonce((n) => n + 1);
          }}
          onExecuteCopy={(target) => {
            if (!copySrc) return;
            setState((prev) => applyDirectCopy(prev, config, copySrc, target));
            setFlashNonce((n) => n + 1);
          }}
        />
      )}

      {/* PC 指令轨道 */}
      {hasPcTrack && (
        <PcTrackDesk
          config={config}
          caseIndex={state.caseIndex}
          pcIndex={state.pcIndex}
          halted={state.halted}
          regs={state.regs}
          onSwitchCase={(idx) => handleReset(idx)}
          onStep={(choice) => {
            setState((prev) => applyPcTrackStep(prev, config, choice));
            setFlashNonce((n) => n + 1);
          }}
        />
      )}

      {/* 仅在触发警告/防错时弹出简洁气泡，平时保持视觉纯净 */}
      {state.feedback?.tone === 'warn' && (
        <div className="rounded-xl border border-amber-500/60 bg-amber-950/40 px-3 py-2 text-xs text-amber-200">
          {state.feedback.message}
        </div>
      )}

      {/* 紧凑汇编录制带（达成时边框变绿 + 单一「去编码」按钮） */}
      {config.recordToAsm !== false && (
        <div
          className={`rounded-xl border p-2.5 transition ${
            state.completed
              ? 'border-emerald-400/70 bg-emerald-950/25'
              : 'border-slate-800 bg-slate-950/80'
          }`}
        >
          <div className="flex items-center justify-between gap-2">
            <span className="inline-flex items-center gap-1.5 font-mono text-[11px] font-semibold text-slate-400">
              <Terminal className="h-3.5 w-3.5 text-emerald-400" />
              ASM ({state.recordings.length})
            </span>
            {state.completed && onFocusEditor && (
              <button
                type="button"
                onClick={onFocusEditor}
                className="inline-flex items-center gap-1 rounded-lg bg-emerald-400 px-2.5 py-1 text-xs font-bold text-slate-950 hover:bg-emerald-300"
              >
                去编码 <ArrowRight className="h-3.5 w-3.5" />
              </button>
            )}
          </div>
          {state.recordings.length > 0 && (
            <div className="mt-1.5 max-h-28 space-y-1 overflow-y-auto font-mono text-xs">
              {state.recordings.map((step, i) => (
                <div
                  key={`${i}-${step.asm}`}
                  className="rounded bg-slate-900/90 px-2 py-0.5 text-cyan-300"
                >
                  <span className="mr-2 text-slate-600">{i + 1}</span>
                  {step.asm}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
