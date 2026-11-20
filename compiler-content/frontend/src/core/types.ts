// =========================================================================
//  Shared type definitions
//
//  The `...Dto` types below mirror the Rust structs in
//  `app/src-tauri/src/dto.rs` (all camelCase). If you change one side,
//  change the other — there is no codegen here.
// =========================================================================

/** Parse result for a ```riscv fenced block */
export interface RiscvBlock {
  config: EditorConfig | null;
  code: string;
  error: string | null;
}

/** Source language. `asm` = 星之语 (assembly), `c` = 星咒 (C subset). */
export type Language = 'asm' | 'c';

/** Editor configuration from the JSON preamble in a ```riscv block */
export interface EditorConfig {
  level: string;
  language?: Language;
  lineNumbers?: boolean;
  constraints?: Constraints;
  toolbar?: ToolbarConfig;
}

/** Line-level constraints applied at build time */
export interface Constraints {
  /** 1-based line numbers; a negative number counts from the end (-1 = last line) */
  readonly?: number[];
  hidden?: number[];
  suggested?: Record<string, string>; // line -> placeholder text
  highlight?: number[];
  /** First N lines read-only (region form, so inserting lines stays safe) */
  readonlyPrefix?: number;
  /** Last N lines read-only */
  readonlySuffix?: number;
}

/** Toolbar button visibility toggles */
export interface ToolbarConfig {
  run?: boolean;
  debug?: boolean;
  submit?: boolean;
  load?: boolean;
  save?: boolean;
}

/** Severity level for a console log entry */
export type ConsoleLevel = 'info' | 'success' | 'error' | 'warning' | 'empty';

/** A single message appended to the console output area */
export interface ConsoleEntry {
  level: ConsoleLevel;
  message: string;
  html?: boolean;
}

/** Lifecycle states for the debugger FSM */
export type DebugState = 'IDLE' | 'RUNNING' | 'PAUSED' | 'STEPPING' | 'STEPPING_BACK';

/** Register snapshot indexed by register name (e.g. "x0") */
export type RegisterSnapshot = Record<string, number>;

/** A memory change record from a single step */
export interface MemoryChange {
  address: string;
  bytes: number[];
}

// ── Backend DTOs ───────────────────────────────────────────────────────

/** One entry of the level list — `list_levels` */
export interface LevelSummary {
  id: string;
  chapter: number;
  number: number;
  title: string;
  language: Language;
  passed: boolean;
  /**
   * Whether this level may be entered right now.
   *
   * The rule (level N needs N-1 to be passed, level 1 is always open) lives in the
   * Rust domain layer (`UserProgress::is_unlocked`) and is decided there for the
   * whole list at once — the frontend must NOT re-derive it from `passed`, because
   * a single entry is not enough information.
   */
  unlocked: boolean;
  attempts: number;
  bestScore?: number;
}

/** Everything needed to render a level — `load_level` */
export interface LevelPayload {
  id: string;
  title: string;
  language: Language;
  editor: EditorConfig;
  initialCode: string;
  /** Reference solution. Only reveal it when the player is stuck. */
  solution?: string;
  solved: boolean;
}

/** Machine snapshot after a run / a single step */
export interface MachineSnapshot {
  pc: number;
  /** Source line for the current PC (1-based; 0 = unknown, e.g. inside the boot stub) */
  pcLine: number;
  regs: RegisterSnapshot;
  tick: number;
  instructions: number;
  output: string;
  exitCode?: number;
  /** `exit` / `ebreak` / `mmio_write` / `error` / `limit` / `ready` */
  pauseKind: string;
  pauseDetail: string;
  halted: boolean;
  canStep: boolean;
  canStepBack: boolean;
}

/** `debug_step` / `debug_step_back` result */
export interface StepResult extends MachineSnapshot {
  regChanges: RegisterSnapshot;
  /** address -> new byte value */
  memChanges: Record<string, number>;
}

export interface DiffDto {
  what: string;
  expected: string;
  actual: string;
}

export interface FailureDto {
  caseIndex: number;
  pc: number;
  line: number;
  reason: string;
  reasonText: string;
  regs: RegisterSnapshot;
}

export interface TestVerdictDto {
  name: string;
  passed: boolean;
  detail: string;
  diffs: DiffDto[];
  actualOutput?: string;
  failure?: FailureDto;
}

export interface ErrorDto {
  line: number;
  message: string;
  /** `Assemble` / `Compile` */
  kind: string;
}

/** `submit_code` result */
export interface SubmitResult {
  level: string;
  passed: boolean;
  score: number;
  summary: string;
  tests: TestVerdictDto[];
  errors: ErrorDto[];
  generatedAssembly?: string;
  attempts: number;
  firstClear: boolean;
}

/** Auto-save draft payload (localStorage) */
export interface Draft {
  code: string;
  savedAt: string;
  level: string;
}

/** Constraint handler: applies a constraint to a textarea */
export interface ConstraintHandler {
  (lineEl: HTMLTextAreaElement, index: number, value: unknown, config: EditorConfig): void;
}
