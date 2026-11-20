//! 模拟器主体：取指 → 解码 → 执行，以及暂停条件、系统调用、历史回溯。

use std::collections::HashMap;

use crate::config::{MmioDirection, SimulatorConfig};
use crate::decode::{decode, Instruction};
use crate::history::{History, HistoryEntry};
use crate::memory::{MemError, Memory, MMIO_DISPLAY, MMIO_INPUT, MMIO_MANA};
use crate::{reg_abi_name, reg_index, reg_name};

/// 访问宽度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordSize {
    Byte,
    Half,
    Word,
}

impl WordSize {
    fn bytes(self) -> u32 {
        match self {
            WordSize::Byte => 1,
            WordSize::Half => 2,
            WordSize::Word => 4,
        }
    }
}

/// 暂停原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseReason {
    /// 程序正常结束（`ecall` SYS_EXIT），`code` 为退出码
    Exit { code: i32 },
    /// 断点（`ebreak`）
    Ebreak,
    /// 等待指令（`slp`）
    Sleep { cycles: u32 },
    /// MMIO 输入队列为空时被读
    MmioReadEmpty { addr: u32 },
    /// MMIO 输出被写入
    MmioWrite { addr: u32, value: u32 },
    /// 运行错误
    Error { msg: String, pc: u32 },
    /// 超过指令上限
    Limit,
}

impl PauseReason {
    /// 稳定的字符串标识（用于 IPC 与判定）。
    pub fn kind(&self) -> &'static str {
        match self {
            PauseReason::Exit { .. } => "exit",
            PauseReason::Ebreak => "ebreak",
            PauseReason::Sleep { .. } => "sleep",
            PauseReason::MmioReadEmpty { .. } => "mmio_read_empty",
            PauseReason::MmioWrite { .. } => "mmio_write",
            PauseReason::Error { .. } => "error",
            PauseReason::Limit => "limit",
        }
    }

    /// 是否为失败性暂停（判定时算"测例失败"）。
    pub fn is_failure(&self) -> bool {
        matches!(self, PauseReason::Error { .. } | PauseReason::Limit)
    }

    /// 面向玩家的一句话说明。
    pub fn describe(&self) -> String {
        match self {
            PauseReason::Exit { code } => format!("程序结束（退出码 {}）", code),
            PauseReason::Ebreak => "命中断点 ebreak".to_string(),
            PauseReason::Sleep { cycles } => format!("等待 {} 个周期", cycles),
            PauseReason::MmioReadEmpty { addr } => {
                format!("输入端口 0x{:08X} 已无数据可读", addr)
            }
            PauseReason::MmioWrite { addr, value } => {
                format!("向输出端口 0x{:08X} 写出 {}", addr, value)
            }
            PauseReason::Error { msg, .. } => msg.clone(),
            PauseReason::Limit => "执行指令数超过本关上限（可能是死循环）".to_string(),
        }
    }
}

/// 运行错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecError {
    #[error("模拟器未加载代码")]
    NoCodeLoaded,
    #[error("模拟器已结束，请先 reset")]
    Halted,
    #[error("{0}")]
    Mem(#[from] MemError),
}

/// `step(n)` 的返回。
#[derive(Debug, Clone)]
pub struct StepResult {
    pub pc: u32,
    /// 本次发生变化的寄存器（新值）
    pub reg_changes: HashMap<String, u32>,
    /// 本次发生变化的内存（地址 → 新值，按字节）
    pub mem_changes: HashMap<u32, u8>,
    /// 本次执行的指令数
    pub instructions_executed: u32,
    /// 当前 tick（累计执行的指令数）
    pub tick: u64,
    /// 若因暂停条件停止
    pub pause_reason: Option<PauseReason>,
    /// 可回退的 tick 数
    pub history_len: usize,
    /// 因超出历史容量而**不可**回退的 tick 数
    pub base_tick: u64,
}

/// `step_back(n)` / `rewind_to(tick)` 的返回。
#[derive(Debug, Clone)]
pub struct StepBackResult {
    pub pc: u32,
    /// 被恢复的寄存器（旧值）
    pub reg_changes: HashMap<String, u32>,
    /// 被恢复的内存字节（旧值）
    pub mem_changes: HashMap<u32, u8>,
    pub tick: u64,
    /// 实际回退了几条
    pub reverted: u32,
    pub history_len: usize,
    pub base_tick: u64,
}

/// `run()` 的返回。
#[derive(Debug, Clone)]
pub struct RunResult {
    pub reason: PauseReason,
    pub pc: u32,
    /// 累计 tick
    pub tick: u64,
    /// 本次 run 执行的指令数
    pub instructions_executed: u64,
    /// 非零寄存器终态
    pub final_regs: HashMap<String, u32>,
    /// 非零内存终态（按字节）
    pub final_memory: HashMap<u32, u8>,
    /// MMIO 输出记录
    pub mmio_output: HashMap<u32, Vec<u32>>,
    /// 显示端口上的输出文本
    pub text_output: String,
    /// 退出码（`SYS_EXIT`）
    pub exit_code: Option<i32>,
}

/// 魔导引擎模拟器。
///
/// 长生命周期对象：创建一次，在测例之间用 `reset()` 复用。
pub struct Simulator {
    config: SimulatorConfig,
    regs: [u32; 32],
    pc: u32,
    memory: Memory,
    /// 已加载的机器码（指令字）
    code: Vec<u32>,
    /// 累计执行的指令数
    tick: u64,
    running: bool,
    history: History,
    /// 程序是否已通过 `ecall` 结束
    halted: bool,
    /// 最近一次 `SYS_EXIT` 的退出码
    exit_code: Option<i32>,
    /// 最近一次暂停的原因（调试面板显示"为什么停在这里"用）。
    /// `step()` / `run()` 之后都有值；`reset()` 与倒退会清掉。
    last_pause: Option<PauseReason>,
    /// 每个内存地址最近一次写入的字节（用于 step 的 mem_changes）
    last_mem_write: HashMap<u32, u8>,
    /// 每个寄存器最近一次写入的值（用于 step 的 reg_changes）
    last_reg_write: HashMap<u8, u32>,
    /// 是否发生过"写代码段"（自修改代码，需要回退到读 RAM 取指）
    code_dirty: bool,
    /// 额外代码区（地址, 机器码）——用于把引导/桩代码放在主代码区之外
    extra_code: Vec<(u32, Vec<u32>)>,
}

impl Simulator {
    /// 新建模拟器。
    pub fn new() -> Self {
        Self {
            config: SimulatorConfig::new(),
            regs: [0u32; 32],
            pc: 0,
            memory: Memory::new(crate::config::DEFAULT_RAM_SIZE),
            code: Vec::new(),
            tick: 0,
            running: false,
            history: History::new(0),
            halted: false,
            exit_code: None,
            last_pause: None,
            last_mem_write: HashMap::new(),
            last_reg_write: HashMap::new(),
            code_dirty: false,
            extra_code: Vec::new(),
        }
    }

    /// 应用配置。会重建内存并清空已加载的代码。
    pub fn configure(&mut self, config: SimulatorConfig) {
        self.config = config;
        self.rebuild_machine();
    }

    /// 取当前配置。
    pub fn config(&self) -> &SimulatorConfig {
        &self.config
    }

    /// 加载机器码（指令字序列）。
    pub fn load_binary(&mut self, words: &[u32]) -> Result<(), ExecError> {
        if words.is_empty() {
            return Err(ExecError::NoCodeLoaded);
        }
        self.code = words.to_vec();
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.memory.write_bytes(self.config.initial_pc, &bytes)?;
        self.halted = false;
        self.exit_code = None;
        self.last_pause = None;
        self.tick = 0;
        self.history.clear();
        self.running = true;
        self.code_dirty = false;
        Ok(())
    }

    /// 是否可继续执行。
    pub fn is_running(&self) -> bool {
        self.running && !self.halted
    }

    /// 是否已结束（`ecall`）。
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// 重置到 `load_binary` 之后的状态。
    ///
    /// **不清空 MMIO 队列与输出记录**——它们是"外部世界"的状态，不属于 CPU。
    /// 因此反复 `reset` + `run` 会一直读到同一批预填输入。
    pub fn reset(&mut self) {
        let code = std::mem::take(&mut self.code);
        let extra = std::mem::take(&mut self.extra_code);
        let queues = self.memory.export_inputs();
        let outputs = self.memory.all_mmio_outputs();
        self.rebuild_machine();
        self.code = code;
        self.extra_code = extra;
        let bytes: Vec<u8> = self.code.iter().flat_map(|w| w.to_le_bytes()).collect();
        let _ = self.memory.write_bytes(self.config.initial_pc, &bytes);
        for (addr, words) in &self.extra_code {
            let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            let _ = self.memory.write_bytes(*addr, &bytes);
        }
        self.memory.restore_inputs(queues);
        self.memory.restore_outputs(outputs);
    }

    /// 重建寄存器 / 内存 / 历史到初始状态（内部使用）。
    fn rebuild_machine(&mut self) {
        let cfg = self.config.clone();
        self.memory = Memory::new(cfg.ram_size);
        self.memory.map_mmio(MMIO_DISPLAY, MmioDirection::Output);
        self.memory.map_mmio(MMIO_INPUT, MmioDirection::Input);
        self.memory.map_mmio(MMIO_MANA, MmioDirection::Input);
        for (&addr, mapping) in &cfg.mmio_map {
            self.memory.map_mmio(addr, mapping.direction);
        }
        for (&addr, &byte) in &cfg.initial_memory {
            self.memory.write_byte(addr, byte);
        }
        self.regs = [0u32; 32];
        for (name, &val) in &cfg.initial_regs {
            if let Some(i) = reg_index(name) {
                if i != 0 {
                    self.regs[i] = val;
                }
            }
        }
        if !cfg.initial_regs.keys().any(|k| k == "sp" || k == "x2") {
            self.regs[2] = cfg.initial_sp;
        }
        self.pc = cfg.initial_pc;
        self.history =
            History::new(if cfg.record_history { cfg.history_capacity } else { 0 });
        self.tick = 0;
        self.halted = false;
        self.exit_code = None;
        self.last_pause = None;
        self.running = false;
        self.code_dirty = false;
        self.last_mem_write.clear();
        self.last_reg_write.clear();
    }

    // ── 状态查询 ──

    pub fn get_pc(&self) -> u32 {
        self.pc
    }

    /// 累计执行指令数。
    pub fn get_tick(&self) -> u64 {
        self.tick
    }

    /// 最近一次暂停的原因（没暂停过 / 正在跑时为 `None`）。
    ///
    /// 调试面板靠它显示"为什么停在这里"。
    pub fn last_pause(&self) -> Option<&PauseReason> {
        self.last_pause.as_ref()
    }

    /// 程序结束时的退出码（`SYS_EXIT` 的 `a0`；没结束为 `None`）。
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// 历史回溯的最早 tick —— 比它更早的状态已经因为容量上限被丢掉了。
    ///
    /// 前端用它判断"还能不能继续往回退"。
    pub fn history_base_tick(&self) -> u64 {
        self.history.base_tick()
    }

    pub fn get_reg(&self, name: &str) -> Option<u32> {
        reg_index(name).map(|i| self.regs[i])
    }

    /// 全部寄存器（含零值，便于 UI 直接渲染网格）。
    pub fn get_all_regs(&self) -> HashMap<String, u32> {
        (0..32).map(|i| (reg_name(i), self.regs[i])).collect()
    }

    /// 只取非零寄存器（紧凑传输）。
    pub fn get_regs_nonzero(&self) -> HashMap<String, u32> {
        (0..32).filter(|&i| self.regs[i] != 0).map(|i| (reg_name(i), self.regs[i])).collect()
    }

    pub fn get_mem(&self, addr: u32, size: u32) -> Vec<u8> {
        self.memory.read_bytes(addr, size)
    }

    /// 读取 C 字符串（到 `\0` 或 `max_len` 为止）。
    pub fn get_cstr(&self, addr: u32, max_len: u32) -> String {
        let bytes = self.memory.read_bytes(addr, max_len);
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).to_string()
    }

    /// 非零内存快照。
    pub fn get_memory_snapshot(&self) -> HashMap<u32, u8> {
        self.memory.snapshot_nonzero().into_iter().collect()
    }

    /// MMIO 输出（原始 u32 序列）。
    pub fn mmio_output(&self, addr: u32) -> Vec<u32> {
        self.memory.mmio_output(addr).to_vec()
    }

    /// 显示端口上的全部输出，按写入顺序拼成文本。
    pub fn text_output(&self) -> String {
        let bytes = self.memory.mmio_output_bytes(MMIO_DISPLAY);
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// 全部 MMIO 输出。
    pub fn all_mmio_output(&self) -> HashMap<u32, Vec<u32>> {
        self.memory.all_mmio_outputs()
    }

    /// 已消耗的周期数（= 已执行指令数 + `slp` 等待的周期）。
    pub fn cycles(&self) -> u64 {
        self.tick
    }

    // ── 状态修改 ──

    pub fn set_reg(&mut self, name: &str, value: u32) -> Result<(), String> {
        let idx = reg_index(name).ok_or_else(|| format!("未知寄存器：{}", name))?;
        if idx != 0 {
            self.regs[idx] = value;
        }
        Ok(())
    }

    pub fn set_mem(&mut self, addr: u32, values: &[u8]) {
        for (i, &b) in values.iter().enumerate() {
            self.memory.write_byte(addr + i as u32, b);
        }
    }

    /// 直接设置 PC（调试器与"跳到指定入口"的场景用）。
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    /// 把一个独立的机器码块装到指定地址，并把它作为"额外代码区"加入取指视图。
    ///
    /// 用于"在玩家代码之外再塞一段引导/桩代码"的场景（例如编译器测试的调用桩）。
    /// 与 `load_binary` 不同，它**不会**改动主代码区，也不会改 PC。
    pub fn load_binary_at(&mut self, addr: u32, words: &[u32]) -> Result<(), ExecError> {
        if words.is_empty() {
            return Err(ExecError::NoCodeLoaded);
        }
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.memory.write_bytes(addr, &bytes)?;
        // 取指时优先看主代码区；主代码区没覆盖到的地址会回落到 RAM，所以这里只需写入内存。
        // 但如果之前发生过"写代码段"，fetch 会一直走 RAM，因此这里顺手清掉该标记。
        self.code_dirty = false;
        self.extra_code.push((addr, words.to_vec()));
        Ok(())
    }

    /// 写入 C 字符串（含结尾 `\0`）。
    pub fn set_cstr(&mut self, addr: u32, text: &str) {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        self.set_mem(addr, &bytes);
    }

    pub fn push_mmio(&mut self, addr: u32, values: &[u32]) {
        self.memory.push_mmio(addr, values);
    }

    pub fn clear_mmio(&mut self) {
        self.memory.clear_mmio();
    }

    // ── 执行 ──

    /// 执行 n 条指令（或提前因暂停条件停止）。
    pub fn step(&mut self, n: u32) -> Result<StepResult, ExecError> {
        if self.code.is_empty() {
            return Err(ExecError::NoCodeLoaded);
        }
        self.last_mem_write.clear();
        self.last_reg_write.clear();
        let mut executed = 0u32;
        let mut pause: Option<PauseReason> = None;
        for _ in 0..n.max(1) {
            if self.halted {
                pause = Some(PauseReason::Exit { code: self.exit_code.unwrap_or(0) });
                break;
            }
            if self.tick >= self.config.max_instructions {
                pause = Some(PauseReason::Limit);
                break;
            }
            match self.one_instruction()? {
                None => executed += 1,
                Some(reason) => {
                    executed += 1;
                    pause = Some(reason);
                    break;
                }
            }
        }
        self.running = true;
        self.last_reg_write.remove(&0);
        self.last_pause = pause.clone();
        let reg_changes =
            self.last_reg_write.iter().map(|(&i, &v)| (reg_name(i as usize), v)).collect();
        Ok(StepResult {
            pc: self.pc,
            reg_changes,
            mem_changes: self.last_mem_write.clone(),
            instructions_executed: executed,
            tick: self.tick,
            pause_reason: pause,
            history_len: self.history.len(),
            base_tick: self.history.base_tick(),
        })
    }

    /// 全速执行，直到暂停条件触发。
    pub fn run(&mut self) -> Result<RunResult, ExecError> {
        if self.code.is_empty() {
            return Err(ExecError::NoCodeLoaded);
        }
        self.running = true;
        let start_tick = self.tick;

        let reason = loop {
            if self.halted {
                break PauseReason::Exit { code: self.exit_code.unwrap_or(0) };
            }
            if self.tick >= self.config.max_instructions {
                break PauseReason::Limit;
            }
            match self.one_instruction()? {
                None => continue,
                Some(reason) => break reason,
            }
        };

        self.last_pause = Some(reason.clone());
        Ok(RunResult {
            reason: reason.clone(),
            pc: self.pc,
            tick: self.tick,
            instructions_executed: self.tick - start_tick,
            final_regs: self.get_regs_nonzero(),
            final_memory: self.memory.snapshot_nonzero().into_iter().collect(),
            mmio_output: self.memory.all_mmio_outputs(),
            text_output: self.text_output(),
            exit_code: self.exit_code,
        })
    }

    /// 倒退 n 条指令（从历史恢复）。
    pub fn step_back(&mut self, n: u32) -> StepBackResult {
        self.rewind_by(n.max(1) as u64)
    }

    /// 回退到指定 tick（含）。若目标已被淘汰则回退到最老可用点。
    pub fn rewind_to(&mut self, tick: u64) -> StepBackResult {
        let target = tick.max(self.history.base_tick());
        self.undo_to(target)
    }

    fn rewind_by(&mut self, n: u64) -> StepBackResult {
        let target = self.tick.saturating_sub(n).max(self.history.base_tick());
        self.undo_to(target)
    }

    fn undo_to(&mut self, target: u64) -> StepBackResult {
        let mut reg_changes = HashMap::new();
        let mut mem_changes = HashMap::new();
        let mut reverted = 0u32;
        while self.tick > target {
            let Some(entry) = self.history.pop() else { break };
            self.pc = entry.old_pc;
            self.tick = self.tick.saturating_sub(1);
            if let Some((idx, old)) = entry.old_reg {
                self.regs[idx as usize] = old;
                reg_changes.insert(reg_name(idx as usize), old);
            }
            if let Some((addr, old)) = entry.old_mem {
                self.memory.write_byte(addr, old);
                mem_changes.insert(addr, old);
            }
            reverted += 1;
        }
        // 倒退后允许继续执行（哪怕之前已经 exit）
        self.halted = false;
        self.exit_code = None;
        self.last_pause = None;
        self.running = true;
        self.code_dirty = false;
        StepBackResult {
            pc: self.pc,
            reg_changes,
            mem_changes,
            tick: self.tick,
            reverted,
            history_len: self.history.len(),
            base_tick: self.history.base_tick(),
        }
    }

    /// 已加载的代码字（供地址 ↔ 源码行映射）。
    pub fn code_words(&self) -> &[u32] {
        &self.code
    }

    // ── 内部实现 ──

    /// 取一条指令的机器码（优先代码视图，其次 RAM —— 支持自修改代码）。
    fn fetch(&self, addr: u32) -> Option<u32> {
        if addr % 4 != 0 {
            return None;
        }
        let start = self.config.initial_pc;
        if !self.code_dirty && addr >= start {
            let idx = ((addr - start) / 4) as usize;
            if let Some(&w) = self.code.get(idx) {
                return Some(w);
            }
        }
        // 额外代码区（引导/桩代码）
        for (base, words) in &self.extra_code {
            if addr >= *base {
                let idx = ((addr - base) / 4) as usize;
                if let Some(&w) = words.get(idx) {
                    return Some(w);
                }
            }
        }
        if self.memory.in_ram(addr) {
            let bytes = self.memory.read_bytes(addr, 4);
            Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        } else {
            None
        }
    }

    /// 执行一条指令。返回 `Some(reason)` 表示需要暂停。
    fn one_instruction(&mut self) -> Result<Option<PauseReason>, ExecError> {
        let pc = self.pc;
        let Some(word) = self.fetch(pc) else {
            let reason = PauseReason::Error {
                msg: format!("PC 0x{:08X} 处没有指令（越界或未对齐）", pc),
                pc,
            };
            return Ok(self.fail(reason));
        };

        let inst = match decode(word) {
            Ok(i) => i,
            Err(e) => {
                return Ok(self.fail(PauseReason::Error {
                    msg: format!("0x{:08X}: {}", pc, e),
                    pc,
                }))
            }
        };

        // 执行阶段的安全字段检查
        if !self.config.allowed_ops.is_empty() {
            let m = inst.mnemonic();
            if !self.config.allowed_ops.iter().any(|a| a == m) {
                let reason = PauseReason::Error {
                    msg: format!("第 0x{:08X} 条指令 `{}` 不在本关允许的指令内", pc, m),
                    pc,
                };
                return Ok(self.fail(reason));
            }
        }
        if !self.config.forbidden_regs.is_empty() {
            let forbidden: Vec<usize> =
                self.config.forbidden_regs.iter().filter_map(|n| reg_index(n)).collect();
            let touched: Vec<u8> =
                inst.reads().into_iter().chain(inst.writes()).collect();
            if let Some(&bad) = touched.iter().find(|r| forbidden.contains(&(**r as usize))) {
                let reason = PauseReason::Error {
                    msg: format!(
                        "第 0x{:08X} 条指令 `{}` 访问了被禁止的寄存器 {}（{}）",
                        pc,
                        inst.mnemonic(),
                        reg_name(bad as usize),
                        reg_abi_name(bad as usize)
                    ),
                    pc,
                };
                return Ok(self.fail(reason));
            }
        }

        // 记录历史（执行前）：只记录"真正会被覆盖"的寄存器与内存
        let reg_before = inst.writes().first().map(|&r| (r, self.regs[r as usize]));
        let mem_before = self.predict_mem_write(&inst);
        self.history.push(HistoryEntry { tick: self.tick + 1, old_pc: pc, old_reg: reg_before, old_mem: mem_before });

        // 每条指令开始前清掉上一条的系统调用号，避免"上一次是 putc"污染本条的暂停判断
        self.tick += 1;
        self.pc = pc.wrapping_add(4);
        let is_exit = match self.execute(&inst, pc) {
            Ok(v) => v,
            Err(reason) => return Ok(self.fail(reason)),
        };
        Ok(self.check_pause_after(&inst, is_exit))
    }

    /// 预测本条指令会写哪个内存字节（用于历史记录）。
    fn predict_mem_write(&self, inst: &Instruction) -> Option<(u32, u8)> {
        let (rs1, _width) = match *inst {
            Instruction::Sb { rs1, .. } => (rs1, 1),
            Instruction::Sh { rs1, .. } => (rs1, 2),
            Instruction::Sw { rs1, .. } => (rs1, 4),
            _ => return None,
        };
        let imm = match *inst {
            Instruction::Sb { imm, .. } | Instruction::Sh { imm, .. } | Instruction::Sw { imm, .. } => imm,
            _ => return None,
        };
        let addr = self.regs[rs1 as usize].wrapping_add(imm as u32);
        if self.memory.is_mmio(addr) || !self.memory.in_ram(addr) {
            return None;
        }
        Some((addr, self.memory.read_bytes(addr, 1)[0]))
    }

    /// 执行一条已解码指令。
    ///
    /// 返回 `Ok(true)` 表示"本条是程序结束的 `ecall`"（由 `pause_on.ecall` 决定是否暂停）。
    fn execute(&mut self, inst: &Instruction, pc: u32) -> Result<bool, PauseReason> {
        use Instruction::*;
        let regs = self.regs;
        let mut exit_syscall = false;

        macro_rules! set {
            ($rd:expr, $val:expr) => {{
                let rd = $rd as usize;
                if rd != 0 {
                    let v: u32 = $val;
                    self.regs[rd] = v;
                    self.last_reg_write.insert(rd as u8, v);
                }
            }};
        }

        match *inst {
            // ── 寄存器运算 ──
            Add { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize].wrapping_add(regs[rs2 as usize])),
            Sub { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize].wrapping_sub(regs[rs2 as usize])),
            Sll { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize] << (regs[rs2 as usize] & 31)),
            Slt { rd, rs1, rs2 } => {
                set!(rd, ((regs[rs1 as usize] as i32) < (regs[rs2 as usize] as i32)) as u32)
            }
            Sltu { rd, rs1, rs2 } => set!(rd, (regs[rs1 as usize] < regs[rs2 as usize]) as u32),
            Xor { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize] ^ regs[rs2 as usize]),
            Srl { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize] >> (regs[rs2 as usize] & 31)),
            Sra { rd, rs1, rs2 } => {
                set!(rd, ((regs[rs1 as usize] as i32) >> (regs[rs2 as usize] & 31)) as u32)
            }
            Or { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize] | regs[rs2 as usize]),
            And { rd, rs1, rs2 } => set!(rd, regs[rs1 as usize] & regs[rs2 as usize]),

            // ── M 扩展 ──
            Mul { rd, rs1, rs2 } => set!(
                rd,
                (regs[rs1 as usize] as i32).wrapping_mul(regs[rs2 as usize] as i32) as u32
            ),
            Div { rd, rs1, rs2 } => {
                let a = regs[rs1 as usize] as i32;
                let b = regs[rs2 as usize] as i32;
                set!(rd, if b == 0 { u32::MAX } else { a.wrapping_div(b) as u32 });
            }
            Divu { rd, rs1, rs2 } => {
                let a = regs[rs1 as usize];
                let b = regs[rs2 as usize];
                set!(rd, if b == 0 { u32::MAX } else { a / b });
            }
            Rem { rd, rs1, rs2 } => {
                let a = regs[rs1 as usize] as i32;
                let b = regs[rs2 as usize] as i32;
                set!(rd, if b == 0 { a as u32 } else { a.wrapping_rem(b) as u32 });
            }
            Remu { rd, rs1, rs2 } => {
                let a = regs[rs1 as usize];
                let b = regs[rs2 as usize];
                set!(rd, if b == 0 { a } else { a % b });
            }

            // ── 立即数运算 ──
            Addi { rd, rs1, imm } => set!(rd, regs[rs1 as usize].wrapping_add(imm as u32)),
            Slti { rd, rs1, imm } => set!(rd, ((regs[rs1 as usize] as i32) < imm) as u32),
            Sltiu { rd, rs1, imm } => set!(rd, (regs[rs1 as usize] < imm as u32) as u32),
            Xori { rd, rs1, imm } => set!(rd, regs[rs1 as usize] ^ imm as u32),
            Ori { rd, rs1, imm } => set!(rd, regs[rs1 as usize] | imm as u32),
            Andi { rd, rs1, imm } => set!(rd, regs[rs1 as usize] & imm as u32),
            Slli { rd, rs1, shamt } => set!(rd, regs[rs1 as usize] << (shamt & 31)),
            Srli { rd, rs1, shamt } => set!(rd, regs[rs1 as usize] >> (shamt & 31)),
            Srai { rd, rs1, shamt } => {
                set!(rd, ((regs[rs1 as usize] as i32) >> (shamt & 31)) as u32)
            }

            // ── 加载 ──
            Lb { rd, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Byte)?;
                set!(rd, self.memory.read_byte(addr) as i8 as i32 as u32);
                self.check_mmio_empty(addr)?;
            }
            Lbu { rd, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Byte)?;
                set!(rd, self.memory.read_byte(addr) as u32);
                self.check_mmio_empty(addr)?;
            }
            Lh { rd, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Half)?;
                set!(rd, self.memory.read_half(addr) as i16 as i32 as u32);
                self.check_mmio_empty(addr)?;
            }
            Lhu { rd, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Half)?;
                set!(rd, self.memory.read_half(addr) as u32);
                self.check_mmio_empty(addr)?;
            }
            Lw { rd, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Word)?;
                if addr == MMIO_MANA {
                    let cycles = self.tick as u32;
                    set!(rd, cycles);
                } else {
                    set!(rd, self.memory.read_word(addr));
                    self.check_mmio_empty(addr)?;
                }
            }

            // ── 存储 ──
            Sb { rs2, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Byte)?;
                let v = regs[rs2 as usize];
                self.memory.write_byte(addr, (v & 0xFF) as u8);
                if !self.memory.is_mmio(addr) {
                    self.last_mem_write.insert(addr, (v & 0xFF) as u8);
                    self.note_code_write(addr);
                }
            }
            Sh { rs2, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Half)?;
                let v = regs[rs2 as usize];
                self.memory.write_half(addr, (v & 0xFFFF) as u16);
                if !self.memory.is_mmio(addr) {
                    for i in 0..2 {
                        self.last_mem_write.insert(addr + i, ((v >> (8 * i)) & 0xFF) as u8);
                        self.note_code_write(addr + i);
                    }
                }
            }
            Sw { rs2, rs1, imm } => {
                let addr = self.effective(rs1, imm, WordSize::Word)?;
                let v = regs[rs2 as usize];
                self.memory.write_word(addr, v);
                if !self.memory.is_mmio(addr) {
                    for i in 0..4 {
                        self.last_mem_write.insert(addr + i, ((v >> (8 * i)) & 0xFF) as u8);
                        self.note_code_write(addr + i);
                    }
                }
            }

            // ── 分支 ──
            Beq { rs1, rs2, imm } => {
                if regs[rs1 as usize] == regs[rs2 as usize] {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }
            Bne { rs1, rs2, imm } => {
                if regs[rs1 as usize] != regs[rs2 as usize] {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }
            Blt { rs1, rs2, imm } => {
                if (regs[rs1 as usize] as i32) < (regs[rs2 as usize] as i32) {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }
            Bge { rs1, rs2, imm } => {
                if (regs[rs1 as usize] as i32) >= (regs[rs2 as usize] as i32) {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }
            Bltu { rs1, rs2, imm } => {
                if regs[rs1 as usize] < regs[rs2 as usize] {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }
            Bgeu { rs1, rs2, imm } => {
                if regs[rs1 as usize] >= regs[rs2 as usize] {
                    self.pc = pc.wrapping_add(imm as u32);
                }
            }

            // ── 跳转 ──
            Jal { rd, imm } => {
                let target = pc.wrapping_add(imm as u32);
                set!(rd, pc.wrapping_add(4));
                self.pc = target;
            }
            Jalr { rd, rs1, imm } => {
                let target = regs[rs1 as usize].wrapping_add(imm as u32) & !1;
                set!(rd, pc.wrapping_add(4));
                self.pc = target;
            }
            Lui { rd, imm } => set!(rd, imm as u32),
            Auipc { rd, imm } => set!(rd, pc.wrapping_add(imm as u32)),

            // ── 其他 ──
            Fence => {}
            Ebreak => {} // 暂停判定在 check_pause_after
            Ecall => exit_syscall = self.syscall(pc)?,
            Slp { cycles } => {
                self.tick = self.tick.saturating_add(cycles as u64);
            }
        }
        Ok(exit_syscall)
    }

    /// MMIO 空读检查。
    fn check_mmio_empty(&self, addr: u32) -> Result<(), PauseReason> {
        if self.memory.last_read_was_empty && self.config.pause_on.mmio_read_empty {
            Err(PauseReason::MmioReadEmpty { addr })
        } else {
            Ok(())
        }
    }

    /// 计算有效地址并校验可访问性。
    fn effective(&self, rs1: u8, imm: i32, size: WordSize) -> Result<u32, PauseReason> {
        let addr = self.regs[rs1 as usize].wrapping_add(imm as u32);
        let n = size.bytes();
        if addr % n != 0 {
            return Err(PauseReason::Error {
                msg: format!("未对齐访问：0x{:08X}（{} 字节）", addr, n),
                pc: self.pc,
            });
        }
        // MMIO 区允许
        if addr >= 0xF000_0000 {
            return Ok(addr);
        }
        if addr.checked_add(n - 1).map(|end| end < self.memory.ram_size()).unwrap_or(false) {
            Ok(addr)
        } else {
            Err(PauseReason::Error {
                msg: format!(
                    "段错误：访问 0x{:08X}（RAM 只有 0x0000–0x{:04X}）",
                    addr,
                    self.memory.ram_size() - 1
                ),
                pc: self.pc,
            })
        }
    }

    fn note_code_write(&mut self, addr: u32) {
        if addr >= self.config.initial_pc {
            let idx = ((addr - self.config.initial_pc) / 4) as usize;
            if idx < self.code.len() {
                self.code_dirty = true;
            }
        }
    }

    /// `ecall` 系统调用分发。
    ///
    /// 返回 `Ok(true)` 表示"这是程序结束（`SYS_EXIT`）"，需要按 `pause_on.ecall` 决定暂停。
    /// 其他系统调用（输出、读入）执行完直接继续——否则每输出一个字符都要来回暂停一次。
    fn syscall(&mut self, pc: u32) -> Result<bool, PauseReason> {
        let num = self.regs[17]; // a7
        let a0 = self.regs[10]; // a0
        let is_exit = match num {
            // SYS_EXIT / SYS_EXIT_CODE
            0 | 5 => {
                self.exit_code = Some(a0 as i32);
                self.halted = true;
                true
            }
            // SYS_PUTC
            1 => {
                self.memory.write_byte(MMIO_DISPLAY, (a0 & 0xFF) as u8);
                false
            }
            // SYS_PUTC_INT
            2 => {
                let text = (a0 as i32).to_string();
                for b in text.bytes() {
                    self.memory.write_byte(MMIO_DISPLAY, b);
                }
                false
            }
            // SYS_GETC
            3 => {
                let c = self.memory.read_byte(MMIO_INPUT) as u32;
                self.regs[10] = c;
                self.check_mmio_empty(MMIO_INPUT)?;
                false
            }
            // SYS_GETINT
            4 => {
                let value = self.read_decimal_int();
                self.regs[10] = value as u32;
                false
            }
            // SYS_TIME
            99 => {
                self.regs[10] = self.tick as u32;
                false
            }
            _ => {
                return Err(PauseReason::Error {
                    msg: format!("未知系统调用号 a7={}（第 0x{:08X} 条指令）", num, pc),
                    pc,
                })
            }
        };
        Ok(is_exit)
    }

    /// 从输入端口读一个十进制整数（跳过前导空白，把结束符还回队列）。
    fn read_decimal_int(&mut self) -> i32 {
        let mut sign = 1i32;
        // 跳过空白
        loop {
            let c = self.memory.read_byte(MMIO_INPUT);
            if c == b' ' || c == b'\n' || c == b'\t' || c == b'\r' {
                continue;
            }
            if c == b'-' {
                sign = -1;
            } else if c.is_ascii_digit() {
                self.memory
                    .input_queues
                    .entry(MMIO_INPUT)
                    .or_default()
                    .push_front(c as u32);
            }
            break;
        }
        let mut value: i32 = 0;
        let mut any = false;
        loop {
            let c = self.memory.read_byte(MMIO_INPUT);
            if c.is_ascii_digit() {
                value = value.saturating_mul(10).saturating_add((c - b'0') as i32);
                any = true;
            } else {
                if c != 0 || any {
                    self.memory
                        .input_queues
                        .entry(MMIO_INPUT)
                        .or_default()
                        .push_front(c as u32);
                }
                break;
            }
        }
        value * sign
    }

    /// 执行后检查是否需要暂停。
    fn check_pause_after(&mut self, inst: &Instruction, is_exit_syscall: bool) -> Option<PauseReason> {
        let po = self.config.pause_on;
        match inst {
            // 只有"程序结束"这一种 ecall 才值得暂停；输出/读入类系统调用继续执行，
            // 否则每输出一个字符就要来回暂停一次。
            Instruction::Ecall if po.ecall && is_exit_syscall => {
                Some(PauseReason::Exit { code: self.exit_code.unwrap_or(0) })
            }
            Instruction::Ebreak if po.ebreak => Some(PauseReason::Ebreak),
            Instruction::Slp { cycles } if po.sleep => Some(PauseReason::Sleep { cycles: *cycles }),
            Instruction::Sw { rs2, rs1, imm } if po.mmio_write => {
                let addr = self.regs[*rs1 as usize].wrapping_add(*imm as u32);
                if self.memory.is_mmio(addr) {
                    Some(PauseReason::MmioWrite { addr, value: self.regs[*rs2 as usize] })
                } else {
                    None
                }
            }
            Instruction::Sh { rs2, rs1, imm } if po.mmio_write => {
                let addr = self.regs[*rs1 as usize].wrapping_add(*imm as u32);
                if self.memory.is_mmio(addr) {
                    Some(PauseReason::MmioWrite { addr, value: self.regs[*rs2 as usize] })
                } else {
                    None
                }
            }
            Instruction::Sb { rs2, rs1, imm } if po.mmio_write => {
                let addr = self.regs[*rs1 as usize].wrapping_add(*imm as u32);
                if self.memory.is_mmio(addr) {
                    Some(PauseReason::MmioWrite { addr, value: self.regs[*rs2 as usize] })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// 运行错误：停下并返回原因（错误永远停下，不会静默继续）。
    fn fail(&mut self, reason: PauseReason) -> Option<PauseReason> {
        self.last_pause = Some(reason.clone());
        self.running = false;
        self.halted = false;
        Some(reason)
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
