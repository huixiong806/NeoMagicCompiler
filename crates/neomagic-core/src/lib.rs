//! Neo Magic Compiler 的领域层。
//!
//! 这里放"游戏怎么玩"的全部逻辑：关卡文件怎么读、玩家代码怎么跑、怎么判定过关、
//! 进度怎么存。它**不依赖 Tauri**，可以脱离桌面壳直接单元测试 —— 这是本项目
//! 能保证关卡"一定可解"的关键（见 [`levels::LevelStore::self_check`]）。
//!
//! ```no_run
//! use neomagic_core::{judge, levels::LevelStore};
//!
//! let store = LevelStore::default_repo();
//! let bundle = store.load("1-1").unwrap();
//! let verdict = judge::judge(&bundle.config, "add x3, x1, x2\nli a7, 0\necall\n");
//! assert!(verdict.passed);
//! ```

pub mod config;
pub mod embedded;
pub mod judge;
pub mod levels;
pub mod progress;
pub mod runner;

#[cfg(test)]
mod selfcheck;

pub use config::{CallSpec, Goal, Language, LevelConfig, MemExpect, TestCase};
pub use judge::{judge, Diff, FailureReason, FailureSite, TestVerdict, Verdict};
pub use levels::{EditorConfig, LevelBundle, LevelError, LevelSource, LevelStore};
pub use progress::{LevelProgress, Settings, UserProgress};
pub use runner::{CaseOutcome, CodeError, ErrorKind, MachineState, Runner};
