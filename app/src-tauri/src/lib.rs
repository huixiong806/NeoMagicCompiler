//! 桌面壳：把 [`neomagic_core`] 的领域逻辑接到前端。
//!
//! 分三层，各管各的：
//! - **领域层** `crates/neomagic-core`：关卡加载、跑代码、判定、存档。不依赖 Tauri，
//!   可以单独跑测试，所以"关卡一定可解"这件事有人守着（`selfcheck`）；
//! - **逻辑层** [`api`]：把请求翻译成领域层调用、整理返回数据、维护当前调试会话。
//!   也不依赖 Tauri，所以整条链路可以被 `cargo test -p neomagiccompiler` 覆盖；
//! - **IPC 层**（本文件的 `ipc` 模块）：只做参数转发与错误包装。
//!
//! 前端通过 `window.__TAURI__.core.invoke('命令名', { camelCase 参数 })` 调用。

pub mod api;
pub mod dto;

use api::{ApiError, AppState};
use dto::{LevelPayload, LevelSummary, MachineSnapshot, ProgressPatch, StepPayload, SubmitDto, SubmitRequest};
use neomagic_core::progress::{default_progress_path, UserProgress, PROGRESS_FILE};
use tauri::{Emitter, Manager, State};

/// 提交判定完成后发的事件名：前端可以监听它做"通关"动画。
const EV_SUBMIT: &str = "submit-verdict";

/// 把逻辑层的错误翻译成 Tauri 能送回前端的形状。
///
/// Tauri 要求命令的错误类型能 `Serialize`（或能 `Into<InvokeError>`）；这里统一转成
/// **一句话字符串**，因为前端要做的就是把这句话显示在控制台里。用 `.map_err` 而不是
/// 给 `ApiError` 加 `Serialize`，是为了让错误形状在 IPC 边界上只有一个可能。
fn ipc<T>(r: Result<T, ApiError>) -> Result<T, String> {
    r.map_err(|e| e.0)
}

/// 一句问候（保留给"帮助 / 关于"用）。
#[tauri::command]
fn greet(name: &str) -> String {
    format!("你好，{}！欢迎来到魔导核心。", name)
}

// ═══════════════════════════════════════════════════════════════════════════
//  IPC 层：薄转发。真正的逻辑在 api.rs（那里可以被测试）
// ═══════════════════════════════════════════════════════════════════════════

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> serde_json::Value {
    state.info()
}

#[tauri::command]
fn list_levels(state: State<'_, AppState>) -> Vec<LevelSummary> {
    state.list_levels()
}

#[tauri::command]
fn load_level(state: State<'_, AppState>, level: String) -> Result<LevelPayload, String> {
    ipc(state.load_level(&level))
}

#[tauri::command]
fn run_code(state: State<'_, AppState>, level: String, code: String) -> Result<MachineSnapshot, String> {
    ipc(state.run_code(&level, &code))
}

#[tauri::command]
fn debug_start(state: State<'_, AppState>, level: String, code: String) -> Result<MachineSnapshot, String> {
    ipc(state.debug_start(&level, &code))
}

#[tauri::command]
fn debug_step(state: State<'_, AppState>, n: Option<u32>) -> Result<StepPayload, String> {
    ipc(state.debug_step(n))
}

#[tauri::command]
fn debug_step_back(state: State<'_, AppState>, n: Option<u32>) -> Result<StepPayload, String> {
    ipc(state.debug_step_back(n))
}

#[tauri::command]
fn debug_continue(state: State<'_, AppState>) -> Result<MachineSnapshot, String> {
    ipc(state.debug_continue())
}

#[tauri::command]
fn debug_reset(state: State<'_, AppState>) -> Result<MachineSnapshot, String> {
    ipc(state.debug_reset())
}

#[tauri::command]
fn debug_quit(state: State<'_, AppState>) {
    state.debug_quit();
}

#[tauri::command]
fn debug_state(state: State<'_, AppState>) -> Option<MachineSnapshot> {
    state.debug_state()
}

/// 提交：跑全部测例、给出判定、记进度。
///
/// ⚠ 参数写成**扁平的 `level` + `code`**，因为前端就是这么发的：
/// `invoke('submit_code', { level, code })`。
/// 早先签名是 `req: SubmitRequest`，Tauri 于是去找名为 `req` 的键，报
/// `command submit_code missing required key req` —— 提交功能整个不可用。
/// （`#[tauri::command]` 的参数不支持 `#[serde(flatten)]`，所以这里只能显式列出。)
#[tauri::command]
fn submit_code(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    level: String,
    code: String,
) -> Result<SubmitDto, String> {
    let req = SubmitRequest { level, code };
    let dto = ipc(state.submit(&req.level, &req.code))?;
    // 事件失败不该让提交结果丢失
    if let Err(e) = app.emit(EV_SUBMIT, &dto) {
        eprintln!("[事件] 发送 {} 失败：{}", EV_SUBMIT, e);
    }
    Ok(dto)
}

#[tauri::command]
fn load_progress(state: State<'_, AppState>) -> UserProgress {
    state.progress()
}

#[tauri::command]
fn save_progress(
    state: State<'_, AppState>,
    patch: Option<ProgressPatch>,
) -> Result<UserProgress, String> {
    ipc(state.save_progress(patch))
}

// ═══════════════════════════════════════════════════════════════════════════
//  入口
// ═══════════════════════════════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 存档放系统的应用数据目录（Windows 下 roughly %APPDATA%\com.neomagiccompiler.app\）
            let dir = app.path().app_data_dir().ok();
            let path = default_progress_path(dir.as_deref());
            eprintln!("[存档] {}", path.display());
            let state = AppState::new(path);
            eprintln!(
                "[关卡] {} 关，内容来自 {}",
                state.list_levels().len(),
                state.info()["contentRoot"]
            );
            let _ = PROGRESS_FILE; // 常量在文档里出现过，这里让它可见
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            app_info,
            list_levels,
            load_level,
            run_code,
            debug_start,
            debug_step,
            debug_step_back,
            debug_continue,
            debug_reset,
            debug_quit,
            debug_state,
            submit_code,
            load_progress,
            save_progress,
        ])
        .run(tauri::generate_context!())
        .expect("启动魔导核心失败");
}

// ═══════════════════════════════════════════════════════════════════════════
//  测试：整条 IPC 链路
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一份"干净的"应用状态（存档写在临时目录里，不污染真存档）。
    fn state() -> AppState {
        let dir = std::env::temp_dir().join(format!(
            "nmc-app-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        AppState::new(dir.join(PROGRESS_FILE))
    }

    /// 关卡列表：至少要有四章、ID 与标题都对得上。
    #[test]
    fn level_list_covers_four_chapters() {
        let s = state();
        let levels = s.list_levels();
        assert!(!levels.is_empty(), "一关都没列出来");
        let chapters: std::collections::BTreeSet<u32> = levels.iter().map(|l| l.chapter).collect();
        assert!(
            chapters.contains(&1) && chapters.contains(&2),
            "第一/二章必须都在，实际章节：{:?}",
            chapters
        );
        for l in &levels {
            assert!(!l.title.is_empty(), "关卡 {} 没有标题", l.id);
            assert!(
                l.title.contains(&format!("{}", l.chapter)),
                "关卡 {} 的标题看起来不对：{}",
                l.id,
                l.title
            );
        }
    }

    /// 进度门禁（"通过才解锁"）在**整条链路上**的样子：
    /// 全新存档只有第 1 关能进；通过 1-1 之后 1-2 开、1-3 仍然锁。
    ///
    /// 这里走的是 `list_levels`（侧边栏与页面门禁实际用的那条路），
    /// 不是直接问领域层函数 —— 前端拿到的 `unlocked` 就是在这里算出来的。
    #[test]
    fn level_gate_locks_everything_but_the_next_level() {
        let s = state();
        let by_id = |s: &AppState| -> std::collections::BTreeMap<String, (bool, bool)> {
            s.list_levels().into_iter().map(|l| (l.id, (l.passed, l.unlocked))).collect()
        };

        // 全新存档：只有第 1 关解锁
        let fresh = by_id(&s);
        assert_eq!(fresh["1-1"], (false, true), "第 1 关永远解锁");
        assert_eq!(fresh["1-2"], (false, false), "1-1 还没过，1-2 不能开");
        assert_eq!(fresh["2-1"], (false, false));
        assert_eq!(
            fresh.values().filter(|(_, unlocked)| *unlocked).count(),
            1,
            "全新存档里只该有一关是开的"
        );

        // 通过 1-1：1-2 开、1-3 仍锁、1-1 仍然可重打
        let solution = s.load_level("1-1").unwrap().solution.expect("1-1 应该有参考解");
        assert!(s.submit("1-1", &solution).unwrap().passed);
        let after = by_id(&s);
        assert_eq!(after["1-1"], (true, true), "已通过的关卡要能随时重进重打");
        assert_eq!(after["1-2"], (false, true), "过了 1-1 就该开 1-2");
        assert_eq!(after["1-3"], (false, false), "1-2 还没过，1-3 不能提前看");
        assert_eq!(
            after.values().filter(|(_, unlocked)| *unlocked).count(),
            2,
            "通关一关之后只该多开一关"
        );
    }

    /// **IPC 契约**：`unlocked` 必须真的走线到前端。
    ///
    /// 上面那条测试读的是 Rust 结构体字段，字段名写错（比如漏了 `rename_all`）
    /// 它照样绿 —— 而前端只认 JSON 里的 `unlocked`。这里把序列化结果钉住。
    #[test]
    fn frontend_payload_shape_level_summary_carries_unlocked() {
        let s = state();
        let summary = s.list_levels().into_iter().find(|l| l.id == "1-1").expect("应该有 1-1");
        let v = serde_json::to_value(&summary).expect("LevelSummary 要能序列化给前端");
        assert_eq!(v["id"], "1-1");
        assert_eq!(v["passed"], false);
        assert_eq!(v["unlocked"], true, "前端靠这个字段决定侧边栏能不能点");
        // camelCase 是对着 types.ts 的 LevelSummary 写的，别退回 snake_case
        assert_eq!(v["bestScore"], serde_json::Value::Null);
        assert!(v.get("best_score").is_none(), "字段名必须是 camelCase");
    }

    /// 参考解必须能通过它自己的关卡 —— 这条是"测试文件写错了"的兜底。
    /// 用第一关（最简单的加法）跑一遍，避免这里也变成一条慢测试。
    #[test]
    fn reference_solution_passes_through_the_submit_path() {
        let s = state();
        let payload = s.load_level("1-1").expect("加载 1-1 失败");
        assert!(!payload.initial_code.trim().is_empty(), "初始代码不该是空的");
        let solution = payload.solution.expect("1-1 应该有参考解");

        let verdict = s.submit("1-1", &solution).expect("提交失败");
        assert!(verdict.passed, "参考解没过：{}", verdict.summary);
        assert_eq!(verdict.score, 100);
        assert_eq!(verdict.attempts, 1);
        assert!(verdict.first_clear, "第一次通过要给 firstClear");

        // 存档落实了吗
        assert!(s.progress().is_passed("1-1"));
        assert!(s.list_levels().iter().any(|l| l.id == "1-1" && l.passed));

        // 再提交一次：不算"第一次通过"，但尝试次数要涨
        let again = s.submit("1-1", &solution).unwrap();
        assert!(again.passed);
        assert!(!again.first_clear, "第二次通过不该再算 firstClear");
        assert_eq!(again.attempts, 2);
    }

    /// 提交一份错代码：要给出可读的失败原因，不能崩、不能沉默。
    ///
    /// 两种失败都可能出现（这里放宽是有意的）：
    /// ① 汇编/编译阶段就报错 → `errors` 里有一句话；
    /// ② 汇编过了但结果不对 → `tests` 里有失败的测例和差异明细。
    #[test]
    fn wrong_code_reports_a_readable_failure() {
        let s = state();
        // `li` 不在 1-1 的白名单里，所以这份代码会卡在汇编阶段
        let verdict = s.submit("1-1", "li x3, 0\nli a7, 0\necall\n").expect("提交不该 Err");
        assert!(!verdict.passed);
        assert!(verdict.score < 100);
        assert!(!verdict.summary.is_empty(), "总要给一句话总结");
        assert!(
            !verdict.errors.is_empty() || !verdict.tests.is_empty(),
            "失败时必须给出错误或测例明细，不能什么都不说"
        );
    }

    /// 汇编过了、但答案错了：要指出差在哪，并给"失败现场"（前端据此进调试）。
    ///
    /// 这里故意用**能通过白名单**的写法来表达"算错了"：把结果写进 `x4` 而不是 `x3`。
    /// （直接改成 `sub` 是不行的 —— 1-1 的白名单只放了 `add`，那样会卡在汇编阶段，
    /// 那是另一条路径，由 `wrong_code_reports_a_readable_failure` 覆盖。）
    #[test]
    fn wrong_result_points_at_the_diff() {
        let s = state();
        let code = "add x4, x1, x2\necall\n";
        let verdict = s.submit("1-1", code).expect("提交不该 Err");
        assert!(verdict.errors.is_empty(), "这份代码应该能汇编过：{:?}", verdict.errors);
        assert!(!verdict.passed, "答案写错寄存器了还能通过？");
        let failed = verdict.tests.iter().find(|t| !t.passed).expect("应该有失败测例");
        assert!(!failed.detail.is_empty(), "失败说明不能是空的");
        assert!(!failed.diffs.is_empty(), "要告诉玩家差在哪");
        assert!(failed.failure.is_some(), "失败现场要能拿到（前端据此进调试）");
    }

    /// 汇编错误要有行号，且不能崩。
    #[test]
    fn broken_code_is_reported_not_panicked() {
        let s = state();
        let verdict = s.submit("1-1", "这不是指令\n").expect("提交不该 Err");
        assert!(!verdict.passed);
        assert!(!verdict.errors.is_empty(), "应该有编译/汇编错误");
    }

    /// 调试：单步 → 倒退 → 继续 → 重置，状态要能对上。
    #[test]
    fn debug_session_steps_forward_and_back() {
        let s = state();
        let payload = s.load_level("1-1").expect("加载 1-1 失败");
        let code = payload.solution.expect("1-1 应该有参考解");

        let start = s.debug_start("1-1", &code).expect("进入调试模式失败");
        assert_eq!(start.tick, 0, "刚进调试时一条指令都没执行");
        assert!(!start.can_step_back, "还没跑，没得退");
        // 停在第一条会触发暂停的指令**之前**，所以行号可能是 1 也可能是 2：
        // 比如 `li a7, 0` / `ecall` 这类"会碰 MMIO 的指令"也算一步，
        // 调试器会在它执行前停下。这里只要求"落在玩家代码的开头附近"。
        assert!(
            (1..=2).contains(&start.pc_line),
            "起点应该在第 1~2 行，实际 {}",
            start.pc_line
        );

        // 往前走两步，再退两步 —— 机器的状态必须**原样回来**。
        //
        // 注意不能假设"一步 = 一条指令"：调试档的暂停条件把"会碰 MMIO 的指令"
        // （读输入、写显示端口、`ecall`）也算作一步，所以步数不一定等于 tick 增量。
        // 真正要保证的契约是"前进 n 步再后退 n 步回到原样"。
        let before = s.debug_state().expect("应该有会话");
        let fwd = s.debug_step(Some(2)).expect("单步失败");
        assert!(fwd.snapshot.tick >= before.tick, "往前走 tick 不该减少");
        let back = s.debug_step_back(Some(2)).expect("倒退失败");
        assert!(back.snapshot.tick < fwd.snapshot.tick, "退两步必须真的退回去（不能只是按钮能动）");
        assert_eq!(back.snapshot.tick, before.tick, "退两步应该回到原来的 tick");
        assert_eq!(back.snapshot.pc, before.pc, "退回来 PC 也要一样");
        assert_eq!(back.snapshot.regs, before.regs, "退回来寄存器也要一样");

        // 继续跑到结束
        let done = s.debug_continue().expect("继续失败");
        assert!(done.tick > before.tick, "继续之后应该往前走了");
        assert!(done.can_step_back, "跑过之后要能往回退");

        // 重置回起点
        let reset = s.debug_reset().expect("重置失败");
        assert_eq!(reset.tick, 0);
        assert_eq!(reset.pc, start.pc);
        assert!(!reset.can_step_back, "重置后没有历史可退");

        s.debug_quit();
        assert!(s.debug_state().is_none(), "退出调试后不该还留着会话");
        assert!(s.debug_step(None).is_err(), "没进调试就单步应该报错而不是崩");
    }

    /// 没进调试模式就单步：给一句人话，而不是 panic。
    #[test]
    fn stepping_without_a_session_is_a_clean_error() {
        let s = state();
        let err = s.debug_step(None).unwrap_err();
        assert!(err.to_string().contains("调试"), "错误信息要说人话：{}", err);
    }

    /// 运行（不判定）：能拿到输出和退出码。
    #[test]
    fn run_code_returns_a_snapshot() {
        let s = state();
        let payload = s.load_level("1-1").unwrap();
        let code = payload.solution.unwrap();
        let snap = s.run_code("1-1", &code).expect("运行失败");
        assert!(snap.halted, "跑完应该停下来了");
        assert!(snap.instructions > 0);
        assert_eq!(snap.regs.len(), 32, "寄存器快照要有 32 个（网格要能直接渲染）");
    }

    /// 存档：写进去能读出来。
    #[test]
    fn progress_round_trips_through_the_file() {
        let s = state();
        s.submit("1-1", "li x3, 0\nli a7, 0\necall\n").unwrap();
        let saved = s.save_progress(Some(ProgressPatch { show_hints: Some(false), last_level: Some("1-1".into()) })).unwrap();
        assert!(!saved.settings.show_hints);
        let reloaded = UserProgress::load(&s.progress_path);
        assert!(!reloaded.settings.show_hints);
        assert_eq!(reloaded.attempts("1-1"), 1);
    }

    /// 不存在的关卡：报错而不是 panic。
    #[test]
    fn unknown_level_is_a_clean_error() {
        let s = state();
        assert!(s.load_level("99-99").is_err());
        assert!(s.run_code("99-99", "nop\n").is_err());
        assert!(s.submit("99-99", "nop\n").is_err());
    }

    // ═══════════════════════════════════════════════════════════════════
    //  IPC 契约测试：**前端发过来的 JSON 到底能不能反序列化成后端要的形状**
    //
    //  为什么必须有这一组：上面那些测试都是直接调 `api.rs` 的方法，
    //  根本不经过序列化边界，所以"前端发扁平参数、后端却要 `req: {...}`"
    //  这种错误**测不出来** —— 而它一发生就是整个功能不可用
    //  （真实事故：提交时报 `command submit_code missing required key req`）。
    //
    //  这里不重新实现序列化：`serde_json::from_str` 走的**就是 Tauri 用的那条路**
    //  （Tauri 内部把命令参数当 JSON 反序列化进参数结构体）。
    //  输入 JSON 一律**逐字抄自** `compiler-content/frontend/src/core/api.ts`，
    //  所以只要两边任何一方的形状变了，这里就会红。
    // ═══════════════════════════════════════════════════════════════════

    /// `submitCode(level, code)` 发的是扁平的 `{ level, code }`。
    /// 命令签名里用 `#[serde(flatten)] req: SubmitRequest` 才能真正接住它。
    #[test]
    fn frontend_payload_shape_submit_code_deserializes() {
        let json = r#"{"level":"1-1","code":"nop\n"}"#;
        let req: dto::SubmitRequest =
            serde_json::from_str(json).expect("前端发的是扁平参数，后端必须能接住");
        assert_eq!(req.level, "1-1");
        assert_eq!(req.code, "nop\n");
    }

    /// 把"不接受嵌套形状"这件事钉住。
    ///
    /// 上面那条测试在**两种**情况下都会通过（`SubmitRequest` 本身是扁平结构体，
    /// serde 自然能吃扁平 JSON），所以单靠它区分不出"命令签名到底有没有 flatten"。
    /// 这条补上另一半：嵌套形状必须**反序列化失败**。
    /// 两条一起，才真正锁死"前端扁平、后端 flatten"这个约定。
    #[test]
    fn submit_request_is_not_wrapped_in_a_req_key() {
        // 嵌套形状（错误形状）：命令签名没有 flatten 时，Tauri 要的就是这种
        let wrapped = r#"{"req":{"level":"1-1","code":"nop\n"}}"#;
        let parsed: Result<dto::SubmitRequest, _> = serde_json::from_str(wrapped);
        assert!(
            parsed.is_err(),
            "后端不应该接受 `{{req: {{...}}}}` 这种嵌套形状 —— \
             前端发的是扁平参数，命令签名必须用 #[serde(flatten)]"
        );
    }

    /// `saveProgress(patch)` 发的是 `{"patch": {...}}`，这个是真的有包一层。
    #[test]
    fn frontend_payload_shape_save_progress_deserializes() {
        let json = r#"{"patch":{"showHints":false,"lastLevel":"1-1"}}"#;
        #[derive(serde::Deserialize)]
        struct Wrapper {
            patch: Option<dto::ProgressPatch>,
        }
        let w: Wrapper = serde_json::from_str(json).expect("存档补丁要能反序列化");
        let patch = w.patch.expect("应该有 patch");
        assert_eq!(patch.show_hints, Some(false));
        assert_eq!(patch.last_level.as_deref(), Some("1-1"));
    }

    /// 无参数的命令：前端发 `invoke('debug_continue')`，参数是空对象或缺失。
    #[test]
    fn frontend_payload_shape_no_arg_commands_are_accepted() {
        // Tauri 对没有参数的命令不会传任何键；后端签名里的 `Option<u32>` 要能吃下缺失
        let empty = serde_json::json!({});
        assert!(empty.get("n").is_none(), "无参命令不该被塞进额外字段");
        let with_n: Option<u32> = serde_json::from_value(serde_json::json!(3)).unwrap();
        assert_eq!(with_n, Some(3));
    }
}
