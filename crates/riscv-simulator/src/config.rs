//! 模拟器配置：初始状态、内存映射、安全限制、暂停条件。

use std::collections::{BTreeMap, HashMap};

/// MMIO 映射方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmioDirection {
    /// CPU 读该地址（外部注入数据）
    Input,
    /// CPU 写该地址（外部读取输出）
    Output,
}

/// 单个 MMIO 地址的映射条目。
#[derive(Debug, Clone, Copy)]
pub struct MmioMapping {
    /// 读写方向
    pub direction: MmioDirection,
    /// 数据宽度（字节）。仅用于文档与前端展示，模拟器按访问宽度取值。
    pub width: u32,
}

impl MmioMapping {
    pub fn input(width: u32) -> Self {
        Self { direction: MmioDirection::Input, width }
    }
    pub fn output(width: u32) -> Self {
        Self { direction: MmioDirection::Output, width }
    }
}

/// 暂停条件——执行到这些事件时停下来，由外部决定继续或失败。
#[derive(Debug, Clone, Copy)]
pub struct PauseOn {
    /// 遇 `ebreak`（断点）暂停
    pub ebreak: bool,
    /// 遇 `ecall`（程序结束）暂停
    pub ecall: bool,
    /// MMIO 输入队列为空时被读 → 暂停
    pub mmio_read_empty: bool,
    /// MMIO 输出被写入 → 暂停（fail-fast 判定用）
    pub mmio_write: bool,
    /// 运行错误 → 暂停（错误始终会被记录，此开关只影响是否立即返回）
    pub error: bool,
    /// 超过指令上限 → 暂停
    pub limit: bool,
    /// 遇 `slp`（等待指令）→ 暂停
    pub sleep: bool,
}

impl Default for PauseOn {
    fn default() -> Self {
        Self {
            ebreak: false,
            ecall: true,
            mmio_read_empty: false,
            mmio_write: false,
            error: true,
            limit: true,
            sleep: false,
        }
    }
}

impl PauseOn {
    /// 调试模式：断点、空读、MMIO 写都停下来，便于观察。
    pub fn debug() -> Self {
        Self { ebreak: true, mmio_read_empty: true, mmio_write: true, sleep: true, ..Self::default() }
    }

    /// 判定模式：只关心程序结束与错误/超限。
    pub fn batch() -> Self {
        Self { ebreak: false, ecall: true, mmio_read_empty: false, mmio_write: false, ..Self::default() }
    }
}

/// 默认 RAM 大小：64 KiB。
pub const DEFAULT_RAM_SIZE: u32 = 0x1_0000;

/// 完整模拟器配置。
#[derive(Debug, Clone)]
pub struct SimulatorConfig {
    /// 初始寄存器值，键可用 `x0`–`x31` 或 ABI 名
    pub initial_regs: HashMap<String, u32>,
    /// 初始 PC
    pub initial_pc: u32,
    /// 初始内存（稀疏写入）
    pub initial_memory: BTreeMap<u32, u8>,
    /// MMIO 地址映射
    pub mmio_map: HashMap<u32, MmioMapping>,
    /// 暂停条件
    pub pause_on: PauseOn,
    /// 单次运行的最大指令数
    pub max_instructions: u64,
    /// 执行阶段禁止访问的寄存器（`x` 名或 ABI 名；空 = 全部允许）
    pub forbidden_regs: Vec<String>,
    /// 执行阶段允许的指令助记符（空 = 全部允许）
    pub allowed_ops: Vec<String>,
    /// RAM 字节数
    pub ram_size: u32,
    /// 栈指针初始值（仅当 `initial_regs` 未显式给出 `sp`/`x2` 时用作默认）
    pub initial_sp: u32,
    /// 是否记录执行历史（单步倒退 / 回看需要）
    pub record_history: bool,
    /// 历史环形缓冲容量（条）
    pub history_capacity: usize,
}

impl SimulatorConfig {
    /// 自由运行用的默认配置。
    pub fn new() -> Self {
        Self {
            initial_regs: HashMap::new(),
            initial_pc: 0,
            initial_memory: BTreeMap::new(),
            mmio_map: HashMap::new(),
            pause_on: PauseOn::default(),
            max_instructions: 1_000_000,
            forbidden_regs: Vec::new(),
            allowed_ops: Vec::new(),
            ram_size: DEFAULT_RAM_SIZE,
            initial_sp: 0xF000,
            record_history: false,
            history_capacity: 20_000,
        }
    }

    /// 调试配置：打开断点暂停与历史记录。
    pub fn debug_mode() -> Self {
        let mut cfg = Self::new();
        cfg.pause_on = PauseOn::debug();
        cfg.record_history = true;
        cfg
    }

    /// 取消段错误限制（把所有地址都当作 RAM）——**不使用**，保留给极端场景。
    pub fn with_ram_size(mut self, size: u32) -> Self {
        self.ram_size = size;
        self
    }
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self::new()
    }
}
