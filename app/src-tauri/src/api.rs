//! 桌面壳的**逻辑层**：不依赖 Tauri，可以被单元测试直接调用。
//!
//! 为什么要把逻辑从 `#[tauri::command]` 里搬出来：Tauri 命令的入参是
//! `State<'_, T>`，测试里造不出来，于是所有 IPC 逻辑都变成"只能靠手点游戏验证"。
//! 现在命令只是薄薄一层转发（见 `lib.rs` 末尾的 `ipc` 模块），真正的逻辑在这里，
//! `cargo test -p neomagiccompiler` 就能把整条链路跑一遍。

use crate::dto::*;
use neomagic_core::judge::Verdict;
use neomagic_core::levels::LevelStore;
use neomagic_core::progress::UserProgress;
use neomagic_core::runner::BOOT_ADDR;
use neomagic_core::{Goal, LevelConfig, Runner, TestCase};
use riscv_simulator::Simulator;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// IPC 层的错误：一句话说清"哪里不对"，前端直接显示给玩家。
#[derive(Debug)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ApiError {}

impl From<String> for ApiError {
    fn from(s: String) -> Self {
        ApiError(s)
    }
}
impl From<&str> for ApiError {
    fn from(s: &str) -> Self {
        ApiError(s.to_string())
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// 应用状态：关卡目录 + 存档 + 当前调试会话。
pub struct AppState {
    pub(crate) store: LevelStore,
    pub(crate) progress: Mutex<UserProgress>,
    pub(crate) progress_path: PathBuf,
    /// 当前调试会话（一台停在某一步的机器）
    pub(crate) debug: Mutex<Option<DebugSession>>,
}

/// 一次调试会话。
pub(crate) struct DebugSession {
    pub(crate) sim: Simulator,
    /// 玩家代码的"地址 → 源码行"对照，用来在面板上显示"现在执行到第几行"
    pub(crate) runner: Runner,
}

impl AppState {
    pub fn new(progress_path: PathBuf) -> Self {
        Self {
            // 开发时读仓库里的关卡文件（改完刷新就能看到），
            // 装好的游戏里读编译期嵌入的那份。
            store: LevelStore::prefer_repo_or_embedded(),
            progress: Mutex::new(UserProgress::load(&progress_path)),
            progress_path,
            debug: Mutex::new(None),
        }
    }

    /// 全部关卡（含通关状态与**解锁状态**），按章节顺序。侧边栏/关卡列表用它。
    ///
    /// 解锁在这里算、而不是在前端算：前端一次只看得到一项，
    /// 而"第 N 关要第 N-1 关通过"是**按顺序**的判定（真正的规则在
    /// `UserProgress::is_unlocked`，那里被单元测试覆盖）。
    pub fn list_levels(&self) -> Vec<LevelSummary> {
        let progress = self.progress.lock().unwrap();
        let ids = self.store.level_ids();
        // `level_ids()` 已经是排好序的，正是解锁判定需要的顺序
        let unlocked = progress.unlocked_levels(&ids);
        ids.into_iter()
            .filter_map(|id| {
                let cfg = self.store.load_config(&id).ok()?;
                let source = self.store.load_source(&id).ok()?;
                let (chapter, number) = split_id(&id);
                let rec = progress.levels.get(&id);
                Some(LevelSummary {
                    title: extract_title(&source.page_text, &id),
                    id: id.clone(),
                    chapter,
                    number,
                    language: cfg.language,
                    passed: rec.map(|r| r.passed).unwrap_or(false),
                    unlocked: unlocked.iter().any(|u| u == &id),
                    attempts: rec.map(|r| r.attempts).unwrap_or(0),
                    best_score: rec.and_then(|r| r.best_score),
                })
            })
            .collect()
    }

    /// 加载一关：编辑器配置 + 初始代码 + 是否已通过。
    pub fn load_level(&self, level: &str) -> ApiResult<LevelPayload> {
        let bundle = self.bundle(level)?;
        Ok(LevelPayload {
            id: level.to_string(),
            title: extract_title(&bundle.source.page_text, level),
            language: bundle.config.language,
            editor: bundle.source.editor,
            initial_code: bundle.source.initial_code,
            solution: bundle.solution,
            solved: self.progress.lock().unwrap().is_passed(level),
        })
    }

    pub(crate) fn bundle(&self, level: &str) -> ApiResult<neomagic_core::LevelBundle> {
        self.store
            .load(level)
            .map_err(|e| ApiError(format!("加载关卡 {} 失败：{}", level, e)))
    }

    /// 跑一下玩家代码（**不判定**），用这一关的第一条测例当默认场景。
    ///
    /// "运行"的语义是"我想看看它怎么跑"，所以它不报对错；对错交给"提交"。
    pub fn run_code(&self, level: &str, code: &str) -> ApiResult<MachineSnapshot> {
        let bundle = self.bundle(level)?;
        let runner = make_runner(bundle.config.clone(), code)?;
        let case = default_case(&bundle.config);
        let outcome = runner.run_case(&case);
        Ok(MachineSnapshot {
            pc: outcome.state.pc,
            pc_line: runner.line_at(outcome.state.pc),
            regs: outcome.state.regs.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            tick: outcome.state.instructions,
            instructions: outcome.state.instructions,
            output: outcome.state.output.clone(),
            exit_code: outcome.state.exit_code,
            pause_kind: outcome.state.pause_kind.clone(),
            pause_detail: outcome.state.pause_detail.clone().unwrap_or_default(),
            halted: true,
            can_step: false,
            can_step_back: false,
        })
    }

    /// 进入调试模式：造一台停在起点的机器，返回初始快照。
    pub fn debug_start(&self, level: &str, code: &str) -> ApiResult<MachineSnapshot> {
        let bundle = self.bundle(level)?;
        let runner = make_runner(bundle.config.clone(), code)?;
        let case = default_case(&bundle.config);
        let sim = runner.debug_simulator(&case);
        let snap = snapshot(&sim, &runner, 0);
        *self.debug.lock().unwrap() = Some(DebugSession { sim, runner });
        Ok(snap)
    }

    /// 单步 / 倒退 / 继续 / 重置 —— 都是"推进当前调试会话"。
    pub fn debug_step(&self, n: Option<u32>) -> ApiResult<StepPayload> {
        self.with_session(|s| {
            let r = s.sim.step(n.unwrap_or(1).max(1)).map_err(|e| ApiError(e.to_string()))?;
            Ok(StepPayload {
                snapshot: snapshot(&s.sim, &s.runner, s.sim.history_base_tick()),
                reg_changes: r.reg_changes.into_iter().collect(),
                mem_changes: r.mem_changes.into_iter().take(64).collect(),
            })
        })
    }

    pub fn debug_step_back(&self, n: Option<u32>) -> ApiResult<StepPayload> {
        self.with_session(|s| {
            let r = s.sim.step_back(n.unwrap_or(1).max(1));
            Ok(StepPayload {
                snapshot: snapshot(&s.sim, &s.runner, s.sim.history_base_tick()),
                reg_changes: r.reg_changes.into_iter().collect(),
                mem_changes: r.mem_changes.into_iter().take(64).collect(),
            })
        })
    }

    /// 继续执行，直到下一个暂停点或程序结束。
    pub fn debug_continue(&self) -> ApiResult<MachineSnapshot> {
        self.with_session(|s| {
            s.sim.run().map_err(|e| ApiError(e.to_string()))?;
            Ok(snapshot(&s.sim, &s.runner, s.sim.history_base_tick()))
        })
    }

    /// 重置：机器回到起点，历史清空。
    pub fn debug_reset(&self) -> ApiResult<MachineSnapshot> {
        self.with_session(|s| {
            s.sim.reset();
            Ok(snapshot(&s.sim, &s.runner, s.sim.history_base_tick()))
        })
    }

    /// 退出调试模式（把机器扔掉）。
    pub fn debug_quit(&self) {
        *self.debug.lock().unwrap() = None;
    }

    /// 当前调试快照（前端刷新后重新接上会话用）。
    pub fn debug_state(&self) -> Option<MachineSnapshot> {
        let guard = self.debug.lock().unwrap();
        guard.as_ref().map(|s| snapshot(&s.sim, &s.runner, s.sim.history_base_tick()))
    }

    fn with_session<T>(&self, f: impl FnOnce(&mut DebugSession) -> ApiResult<T>) -> ApiResult<T> {
        let mut guard = self.debug.lock().unwrap();
        let Some(session) = guard.as_mut() else {
            return Err(ApiError("还没有进入调试模式（先按「调试」）".into()));
        };
        f(session)
    }

    /// 提交：跑全部测例、给出判定、记进度。
    pub fn submit(&self, level: &str, code: &str) -> ApiResult<SubmitDto> {
        let bundle = self.bundle(level)?;
        let verdict = neomagic_core::judge(&bundle.config, code);

        let (attempts, first_clear) = {
            let mut progress = self.progress.lock().unwrap();
            let was_passed = progress.is_passed(level);
            let rec = progress.record(level, verdict.passed, verdict.score);
            let attempts = rec.attempts;
            let first = verdict.passed && !was_passed;
            // 存档写失败不该让玩家看不到判定结果，所以只记日志
            if let Err(e) = progress.save(&self.progress_path) {
                eprintln!("[存档] 写入失败（判定结果仍然返回）：{}", e);
            }
            (attempts, first)
        };

        Ok(to_submit_dto(level, verdict, attempts, first_clear))
    }

    /// 读存档。
    pub fn progress(&self) -> UserProgress {
        self.progress.lock().unwrap().clone()
    }

    /// 写存档（设置变更、显式保存）。
    pub fn save_progress(&self, patch: Option<ProgressPatch>) -> ApiResult<UserProgress> {
        let mut progress = self.progress.lock().unwrap();
        if let Some(p) = patch {
            if let Some(show) = p.show_hints {
                progress.settings.show_hints = show;
            }
            if let Some(last) = p.last_level {
                // 只记日志：关卡顺序由 mdBook 目录决定，不靠存档里的"上次"
                eprintln!("[进度] 上次玩到 {}", last);
            }
        }
        progress.save(&self.progress_path).map_err(|e| ApiError(e.to_string()))?;
        Ok(progress.clone())
    }

    /// 关于对话框 / 诊断用。
    pub fn info(&self) -> serde_json::Value {
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "levels": self.store.level_ids().len(),
            "contentRoot": self.store.content_root().display().to_string(),
            "progressFile": self.progress_path.display().to_string(),
            "embeddedFiles": neomagic_core::embedded::count(),
            "stackTop": neomagic_core::runner::DEFAULT_STACK_TOP,
            "bootAddr": BOOT_ADDR,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  辅助
// ═══════════════════════════════════════════════════════════════════════════

/// 从关卡页里抠出标题：第一个 `## ` 开头的行。
fn extract_title(page_text: &str, id: &str) -> String {
    for line in page_text.lines() {
        if let Some(rest) = line.trim().strip_prefix("## ") {
            return rest.trim().to_string();
        }
    }
    format!("关卡 {}", id)
}

fn split_id(id: &str) -> (u32, u32) {
    let mut it = id.split('-');
    let a = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b)
}

fn make_runner(config: LevelConfig, code: &str) -> ApiResult<Runner> {
    Runner::new(config, code).map_err(|errs| {
        let first = errs
            .first()
            .map(|e| {
                if e.line == 0 {
                    e.message.clone()
                } else {
                    format!("第 {} 行：{}", e.line, e.message)
                }
            })
            .unwrap_or_else(|| "代码没有编译通过".to_string());
        ApiError(first)
    })
}

/// 这一关的"默认场景"：第一条测例（没有测例就造一条空的）。
fn default_case(config: &LevelConfig) -> TestCase {
    match &config.goal {
        Goal::Static { tests } | Goal::Output { tests } => {
            tests.first().cloned().unwrap_or_default()
        }
        // 随机判定的关卡没有固定测例：给一条只有输入的场景，让玩家能调
        Goal::Scripted { .. } => TestCase { name: "调试场景".into(), ..Default::default() },
    }
}

/// 把模拟器当前状态拍成快照。
fn snapshot(sim: &Simulator, runner: &Runner, base_tick: u64) -> MachineSnapshot {
    let pc = sim.get_pc();
    let tick = sim.get_tick();
    let halted = sim.is_halted();
    let (kind, detail) = match sim.last_pause() {
        Some(p) => (p.kind().to_string(), p.describe()),
        None => ("ready".to_string(), "等待开始".to_string()),
    };
    MachineSnapshot {
        pc,
        pc_line: runner.line_at(pc),
        regs: sim.get_all_regs().into_iter().collect::<BTreeMap<_, _>>(),
        tick,
        instructions: tick,
        output: sim.text_output(),
        exit_code: sim.exit_code(),
        pause_kind: kind,
        pause_detail: detail,
        halted,
        can_step: !halted,
        can_step_back: tick > base_tick,
    }
}

fn to_submit_dto(level: &str, verdict: Verdict, attempts: u32, first_clear: bool) -> SubmitDto {
    SubmitDto {
        level: level.to_string(),
        passed: verdict.passed,
        score: verdict.score,
        summary: verdict.summary,
        tests: verdict
            .tests
            .into_iter()
            .map(|t| TestVerdictDto {
                name: t.name,
                passed: t.passed,
                detail: t.detail,
                diffs: t
                    .diffs
                    .into_iter()
                    .map(|d| DiffDto { what: d.what, expected: d.expected, actual: d.actual })
                    .collect(),
                actual_output: t.actual_output,
                failure: t.failure_site.map(|f| FailureDto {
                    case_index: f.case_index,
                    pc: f.pc,
                    line: f.line,
                    reason: format!("{:?}", f.reason),
                    reason_text: f.reason.describe().to_string(),
                    regs: f.regs.into_iter().collect(),
                }),
            })
            .collect(),
        errors: verdict
            .errors
            .into_iter()
            .map(|e| ErrorDto { line: e.line, message: e.message, kind: format!("{:?}", e.kind) })
            .collect(),
        generated_assembly: verdict.generated_assembly,
        attempts,
        first_clear,
    }
}
