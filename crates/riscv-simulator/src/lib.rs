//! 魔导引擎（RV32IM 模拟器）
//!
//! 框架无关的纯逻辑库：只接受机器码，执行 RV32IM 子集，维护 CPU 状态、
//! RAM 与 MMIO，并按配置的暂停条件停下来。
//!
//! ```text
//! 取指 → 解码 → 执行 → 写回
//!   ↑                        │
//!   └────── PC / 暂停条件 ────┘
//! ```
//!
//! ## 快速上手
//!
//! ```no_run
//! use riscv_simulator::{Simulator, SimulatorConfig};
//!
//! let mut sim = Simulator::new();
//! sim.configure(SimulatorConfig::new());
//! sim.load_binary(&[0x00100073]); // ebreak
//! let result = sim.run();
//! assert!(result.is_ok());
//! ```

pub mod config;
mod decode;
mod history;
pub mod memory;
mod sim;

pub use config::{MmioDirection, MmioMapping, PauseOn, SimulatorConfig};
pub use decode::{decode, Instruction};
pub use history::HistoryEntry;
pub use memory::{MEMORY_TOP, MMIO_DISPLAY, MMIO_INPUT, MMIO_MANA};
pub use sim::{ExecError, PauseReason, RunResult, Simulator, StepBackResult, StepResult, WordSize};

/// 寄存器名 → 索引（同时接受 `x0`–`x31` 与 ABI 名）。
pub fn reg_index(name: &str) -> Option<usize> {
    let n = name.trim();
    if let Some(rest) = n.strip_prefix('x') {
        if let Ok(i) = rest.parse::<usize>() {
            return if i < 32 { Some(i) } else { None };
        }
    }
    Some(match n {
        "zero" => 0,
        "ra" => 1,
        "sp" => 2,
        "gp" => 3,
        "tp" => 4,
        "t0" => 5,
        "t1" => 6,
        "t2" => 7,
        "s0" | "fp" => 8,
        "s1" => 9,
        "a0" => 10,
        "a1" => 11,
        "a2" => 12,
        "a3" => 13,
        "a4" => 14,
        "a5" => 15,
        "a6" => 16,
        "a7" => 17,
        "s2" => 18,
        "s3" => 19,
        "s4" => 20,
        "s5" => 21,
        "s6" => 22,
        "s7" => 23,
        "s8" => 24,
        "s9" => 25,
        "s10" => 26,
        "s11" => 27,
        "t3" => 28,
        "t4" => 29,
        "t5" => 30,
        "t6" => 31,
        _ => return None,
    })
}

/// 索引 → 标准寄存器名（`x0`–`x31`）。
pub fn reg_name(index: usize) -> String {
    format!("x{}", index)
}

/// 索引 → ABI 寄存器名。
pub fn reg_abi_name(index: usize) -> &'static str {
    const NAMES: [&str; 32] = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4",
        "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4",
        "t5", "t6",
    ];
    NAMES[index.min(31)]
}
