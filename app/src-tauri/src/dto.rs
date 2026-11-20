//! 前端用的数据结构（IPC 的"接口定义"）。
//!
//! 约定：**全部 camelCase**（前端是 TypeScript）。
//! 对应的 TypeScript 类型在 `compiler-content/frontend/src/core/types.ts`，
//! 两边要一起改。

use neomagic_core::levels::EditorConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 关卡列表里的一项。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LevelSummary {
    pub id: String,
    pub chapter: u32,
    pub number: u32,
    pub title: String,
    pub language: neomagic_core::Language,
    pub passed: bool,
    /// 现在能不能进这一关（第 N 关要第 N-1 关通过；第 1 关永远解锁）。
    ///
    /// 由领域层 `UserProgress::is_unlocked` 判定，**前端不要自己按 `passed` 推**：
    /// 解锁是"按关卡顺序"算的，只让前端看一项是推不出来的。
    pub unlocked: bool,
    pub attempts: u32,
    pub best_score: Option<u32>,
}

/// 加载一关之后前端需要的一切。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LevelPayload {
    pub id: String,
    pub title: String,
    pub language: neomagic_core::Language,
    pub editor: EditorConfig,
    pub initial_code: String,
    /// 参考解（作者用来验证关卡可解的那份），前端只在"卡住了"时给玩家看
    pub solution: Option<String>,
    pub solved: bool,
}

/// 一次运行/单步之后的机器快照。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MachineSnapshot {
    pub pc: u32,
    /// 当前 PC 对应的源码行（1-based；0 = 查不到，比如停在引导代码里）
    pub pc_line: u32,
    pub regs: BTreeMap<String, u32>,
    pub tick: u64,
    pub instructions: u64,
    pub output: String,
    pub exit_code: Option<i32>,
    pub pause_kind: String,
    pub pause_detail: String,
    /// 已经结束（正常退出 / 出错 / 超限）
    pub halted: bool,
    /// 还能继续跑
    pub can_step: bool,
    /// 还能往回退
    pub can_step_back: bool,
}

/// 单步结果：在快照之上多给"这一步改了什么"。
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StepPayload {
    #[serde(flatten)]
    pub snapshot: MachineSnapshot,
    /// 有变化的寄存器（新值）
    pub reg_changes: BTreeMap<String, u32>,
    /// 有变化的内存字节（新值），最多 64 条
    pub mem_changes: BTreeMap<u32, u8>,
}

/// 一条测例的判定结果。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TestVerdictDto {
    pub name: String,
    pub passed: bool,
    pub detail: String,
    pub diffs: Vec<DiffDto>,
    pub actual_output: Option<String>,
    /// 失败现场（前端据此自动进调试模式）
    pub failure: Option<FailureDto>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiffDto {
    pub what: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FailureDto {
    pub case_index: usize,
    pub pc: u32,
    pub line: u32,
    /// 机器可读的原因（`Register` / `Memory` / `Output` …）
    pub reason: String,
    /// 给玩家看的原因
    pub reason_text: String,
    pub regs: BTreeMap<String, u32>,
}

/// 提交结果。
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubmitDto {
    pub level: String,
    pub passed: bool,
    pub score: u32,
    pub summary: String,
    pub tests: Vec<TestVerdictDto>,
    /// 汇编/编译阶段的错误（带行号）
    pub errors: Vec<ErrorDto>,
    /// 星咒关卡：编译器生成的星之语文本（给玩家看）
    pub generated_assembly: Option<String>,
    /// 这一关累计尝试次数
    pub attempts: u32,
    /// 是不是"第一次"通过（用来放通关动画）
    pub first_clear: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub line: u32,
    pub message: String,
    /// `Assemble` / `Compile`
    pub kind: String,
}

/// 提交请求。
///
/// ⚠ **这个结构体是"扁平"的** —— 前端发的是 `{ level, code }`，不是 `{ req: {...} }`。
/// `flatten` 就是干这个的：让命令签名里可以写 `req: SubmitRequest` 而不影响线格式。
///
/// 这里踩过一次：前端 `invoke('submit_code', { level, code })`，后端签名是
/// `req: SubmitRequest`，Tauri 于是去找名为 `req` 的键，报
/// `command submit_code missing required key req` —— 提交功能整个不可用。
/// 单元测试抓不到它（那些测试直接调 `api.rs`，根本不经过序列化边界），
/// 所以下面专门有一条 `frontend_payload_shape_*` 测试守着这个契约。
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubmitRequest {
    pub level: String,
    pub code: String,
}

/// 存档写入请求。
#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPatch {
    #[serde(default)]
    pub show_hints: Option<bool>,
    /// 前端记住的"上次玩到哪一关"
    #[serde(default)]
    pub last_level: Option<String>,
}
