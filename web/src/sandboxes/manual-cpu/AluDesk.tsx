import { Copy, Zap } from 'lucide-react';
import type { ManualCpuAluOp, ManualCpuSandboxConfig } from '../../engine/types';
import type { SourceOperand } from './model';

interface AluDeskProps {
  config: ManualCpuSandboxConfig;
  regs: Record<string, number>;
  left: SourceOperand | null;
  right: SourceOperand | null;
  op: ManualCpuAluOp;
  destReg: string;
  activeSlot: 'left' | 'right' | 'copySrc';
  copySrc: SourceOperand | null;
  onSelectSlot: (slot: 'left' | 'right' | 'copySrc') => void;
  onSelectOp: (op: ManualCpuAluOp) => void;
  onSelectDest: (reg: string) => void;
  onDropToSlot: (slot: 'left' | 'right', payload: SourceOperand) => void;
  onExecuteAlu: () => void;
  onExecuteCopy: (dest: string) => void;
}

function parseDragPayload(raw: string): SourceOperand | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object') return null;
    const obj = parsed as Partial<SourceOperand>;
    if (obj.kind === 'reg' && typeof obj.name === 'string') return { kind: 'reg', name: obj.name };
    if (obj.kind === 'const' && obj.card) return { kind: 'const', card: obj.card };
    return null;
  } catch {
    return null;
  }
}

function slotLabel(operand: SourceOperand | null, regs: Record<string, number>): { top: string; val: string } {
  if (!operand) return { top: 'IN', val: '?' };
  if (operand.kind === 'reg') {
    return { top: operand.name, val: String(regs[operand.name] ?? 0) };
  }
  return { top: `#${operand.card.label}`, val: String(operand.card.value) };
}

export function AluDesk({
  config,
  regs,
  left,
  right,
  op,
  destReg,
  activeSlot,
  copySrc,
  onSelectSlot,
  onSelectOp,
  onSelectDest,
  onDropToSlot,
  onExecuteAlu,
  onExecuteCopy,
}: AluDeskProps) {
  const ops = config.aluOps && config.aluOps.length > 0 ? config.aluOps : (['+'] as ManualCpuAluOp[]);
  const writableRegs = config.focusRegs.filter((r) => r !== 'zero');
  const ready = Boolean(left && right);
  const leftView = slotLabel(left, regs);
  const rightView = slotLabel(right, regs);

  const handleDrop = (slot: 'left' | 'right') => (e: React.DragEvent) => {
    e.preventDefault();
    const payload = parseDragPayload(e.dataTransfer.getData('application/x-nmc-operand'));
    if (payload) onDropToSlot(slot, payload);
  };

  return (
    <div className="space-y-2.5">
      {/* 图形化 ALU 逻辑芯片台 */}
      <div className="rounded-2xl border border-slate-800 bg-slate-950/90 p-3.5">
        {/* 上方双输入引脚 */}
        <div className="grid grid-cols-2 gap-4 px-2">
          {(['left', 'right'] as const).map((slot) => {
            const view = slot === 'left' ? leftView : rightView;
            const filled = slot === 'left' ? Boolean(left) : Boolean(right);
            const active = activeSlot === slot;
            return (
              <button
                key={slot}
                type="button"
                onClick={() => onSelectSlot(slot)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={handleDrop(slot)}
                className={`group flex flex-col items-center justify-center rounded-xl border-2 px-3 py-2 font-mono transition ${
                  active
                    ? 'border-cyan-400 bg-cyan-500/15 shadow-[0_0_16px_rgba(34,211,238,0.2)]'
                    : filled
                      ? 'border-cyan-500/50 bg-slate-900'
                      : 'border-dashed border-slate-700 bg-slate-900/50 hover:border-slate-500'
                }`}
              >
                <span className="text-[10px] font-semibold tracking-wider text-slate-400">
                  {view.top}
                </span>
                <span className={`text-lg font-bold ${filled ? 'text-cyan-200' : 'text-slate-600'}`}>
                  {view.val}
                </span>
              </button>
            );
          })}
        </div>

        {/* SVG 导线 + ALU 梯形芯片 */}
        <div className="relative mx-auto my-1 flex flex-col items-center">
          <svg
            viewBox="0 0 320 92"
            className="h-24 w-full max-w-[320px] select-none"
            aria-label="ALU 运算芯片"
          >
            {/* 左/右输入导线 */}
            <path
              d="M 76 0 L 76 16 L 106 24"
              fill="none"
              stroke={left ? '#22d3ee' : '#334155'}
              strokeWidth={left ? 2.5 : 1.5}
              strokeDasharray={left ? undefined : '4 3'}
            />
            <path
              d="M 244 0 L 244 16 L 214 24"
              fill="none"
              stroke={right ? '#22d3ee' : '#334155'}
              strokeWidth={right ? 2.5 : 1.5}
              strokeDasharray={right ? undefined : '4 3'}
            />
            {/* 经典 V 缺口 ALU 梯形外壳 */}
            <polygon
              points="70,22 142,22 160,36 178,22 250,22 222,74 98,74"
              fill={ready ? 'rgba(8, 51, 68, 0.75)' : 'rgba(15, 23, 42, 0.9)'}
              stroke={ready ? '#22d3ee' : '#334155'}
              strokeWidth={ready ? 2 : 1.5}
            />
            {/* 输出导线 */}
            <line
              x1="160"
              y1="74"
              x2="160"
              y2="92"
              stroke={ready ? '#34d399' : '#334155'}
              strokeWidth={ready ? 2.5 : 1.5}
            />
            <text
              x="160"
              y="62"
              textAnchor="middle"
              fill="#64748b"
              fontSize="9"
              fontFamily="ui-monospace, monospace"
              letterSpacing="2"
            >
              ALU
            </text>
          </svg>

          {/* 嵌在芯片中央的运算符切换胶囊 */}
          <div className="absolute top-[30px] flex items-center gap-1 rounded-full border border-slate-700 bg-slate-900/95 p-0.5 shadow">
            {ops.map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => onSelectOp(item)}
                className={`h-7 min-w-[28px] rounded-full px-2 font-mono text-sm font-bold transition ${
                  op === item
                    ? 'bg-cyan-400 text-slate-950 shadow'
                    : 'text-slate-400 hover:bg-slate-800 hover:text-white'
                }`}
              >
                {item}
              </button>
            ))}
          </div>
        </div>

        {/* 输出端：目标寄存器 + 触发按钮 */}
        <div className="flex items-center justify-center gap-2.5">
          <div className="flex items-center gap-1 rounded-xl border border-emerald-500/40 bg-emerald-950/25 px-2.5 py-1.5">
            <span className="font-mono text-[10px] font-bold text-emerald-400">OUT</span>
            <select
              aria-label="ALU 目标寄存器"
              value={destReg}
              onChange={(e) => onSelectDest(e.target.value)}
              className="rounded bg-slate-900 px-2 py-0.5 font-mono text-sm font-bold text-emerald-200 outline-none focus:ring-1 focus:ring-emerald-400"
            >
              {writableRegs.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </div>

          <button
            type="button"
            disabled={!ready}
            onClick={onExecuteAlu}
            className="inline-flex items-center gap-1.5 rounded-xl bg-cyan-400 px-4 py-2 text-xs font-bold text-slate-950 shadow transition hover:bg-cyan-300 disabled:cursor-not-allowed disabled:bg-slate-800 disabled:text-slate-500"
          >
            <Zap className="h-3.5 w-3.5" /> 执行
          </button>
        </div>
      </div>

      {/* 极简搬运通道（仅允许 mv 关启用） */}
      {config.allowCopy && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-slate-800 bg-slate-950/70 px-3 py-2">
          <div className="flex items-center gap-1.5">
            <Copy className="h-3.5 w-3.5 text-amber-400" />
            <button
              type="button"
              onClick={() => onSelectSlot('copySrc')}
              className={`rounded-md border px-2 py-0.5 font-mono text-xs font-semibold transition ${
                activeSlot === 'copySrc'
                  ? 'border-amber-400 bg-amber-500/20 text-amber-200'
                  : 'border-slate-700 bg-slate-900 text-slate-300'
              }`}
            >
              {copySrc ? slotLabel(copySrc, regs).top : '选源'}
            </button>
            <span className="text-xs text-slate-500">→</span>
          </div>
          <div className="flex flex-wrap gap-1">
            {writableRegs.map((target) => (
              <button
                key={target}
                type="button"
                disabled={!copySrc || (copySrc.kind === 'reg' && copySrc.name === target)}
                onClick={() => onExecuteCopy(target)}
                className="rounded-md border border-amber-500/35 bg-amber-500/10 px-2 py-0.5 font-mono text-xs font-semibold text-amber-200 hover:bg-amber-500/25 disabled:opacity-30"
              >
                {target}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
