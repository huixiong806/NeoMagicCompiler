//! **关卡自检**：对仓库里每一个关卡，用它的参考解跑一遍判定。
//!
//! 这是整个项目的质量生命线：
//! - 参考解跑不过 → 说明**测例或参考解写错了**（关卡不可解），必须修；
//! - 如果所有关卡都能用参考解通过，玩家至少有一组已知可行解。
//!
//! 只在关卡内容改动时才会失败，所以它既是测试也是"内容 CI"。

use crate::judge::Verdict;
use crate::levels::LevelStore;

/// 对全部关卡跑自检，返回报告文本。失败时 `Err` 里列出有问题的关卡。
pub fn check_all() -> Result<String, String> {
    let store = LevelStore::default_repo();
    let ids = store.level_ids();
    if ids.is_empty() {
        return Err(format!("在 {} 里没有找到任何关卡", store.content_dir().display()));
    }

    let mut ok = Vec::new();
    let mut bad = Vec::new();
    let mut missing_solution = Vec::new();

    for id in &ids {
        match store.self_check(id) {
            Ok(Verdict { passed: true, summary, .. }) => ok.push(format!("{} ✓ {}", id, summary)),
            Ok(v) => bad.push(format!("{} ✗ {}", id, v.summary)),
            Err(e) => {
                if e.contains("没有参考解") {
                    missing_solution.push(format!("{}：{}", id, e));
                } else {
                    bad.push(format!("{} ✗ {}", id, e));
                }
            }
        }
    }

    if bad.is_empty() && missing_solution.is_empty() {
        let mut report = format!("全部 {} 个关卡自检通过：\n", ok.len());
        for l in &ok {
            report.push_str("  ");
            report.push_str(l);
            report.push('\n');
        }
        Ok(report)
    } else {
        let mut msg = String::new();
        if !bad.is_empty() {
            msg.push_str("以下关卡用参考解**跑不过**（测例或参考解有问题）：\n");
            for l in &bad {
                msg.push_str("  ");
                msg.push_str(l);
                msg.push('\n');
            }
        }
        if !missing_solution.is_empty() {
            msg.push_str("以下关卡缺少参考解：\n");
            for l in &missing_solution {
                msg.push_str("  ");
                msg.push_str(l);
                msg.push('\n');
            }
        }
        Err(msg)
    }
}

#[test]
fn every_level_is_solvable_with_its_reference_solution() {
    match check_all() {
        Ok(report) => println!("{}", report),
        Err(msg) => panic!("{}", msg),
    }
}

/// 只自检指定关卡（逗号分隔，如 `c3_01,c3_02`）。
///
/// 这是给**并行写关卡的作者**准备的口子：
/// 新关卡还没写完之前，全量自检必然失败，作者的注意力会被别人的关卡分散。
/// 用法：`NMC_SELFCHECK=c3_01,c3_02 cargo test -p neomagic-core selfcheck_only -- --nocapture`
#[test]
fn selfcheck_only_listed_levels() {
    let Ok(list) = std::env::var("NMC_SELFCHECK") else {
        // 没设置就跳过（这条测试只在作者手动指定时有意义）
        return;
    };
    let store = LevelStore::default_repo();
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for raw in list.split(',') {
        let id = raw.trim();
        if id.is_empty() {
            continue;
        }
        let id = id.strip_prefix('c').unwrap_or(id);
        let id = id.replace('_', "-");
        match store.self_check(&id) {
            Ok(v) if v.passed => {
                checked += 1;
                println!("{} ✓ {}", id, v.summary);
            }
            Ok(v) => problems.push(format!("{} ✗ {}", id, v.summary)),
            Err(e) => problems.push(format!("{} ✗ {}", id, e)),
        }
    }
    assert!(problems.is_empty(), "这些关卡自检没过：\n  {}", problems.join("\n  "));
    println!("指定关卡全部通过（{} 关）", checked);
}

/// 逐条测例的详细报告：**每一关的每一条测例**都打印"输入 / 期望 / 实际"。
///
/// 为什么需要它（真事）：`self_check` 的 `summary` 只报**第一条**失败测例。
/// 第四章某一关因此把另外**三个** bug 连同一条**写错的期望值**一起藏住了 ——
/// 修掉第一条之后才露出第二条，来回好几轮。
/// 作者调试某几关时应该用这个，而不是只看 summary。
///
/// 用法：`NMC_SELFCHECK=4-4 NMC_SELFCHECK_VERBOSE=1 cargo test -p neomagic-core selfcheck_verbose -- --nocapture`
#[test]
fn selfcheck_verbose_reports_every_case() {
    let Ok(list) = std::env::var("NMC_SELFCHECK") else { return };
    if std::env::var("NMC_SELFCHECK_VERBOSE").is_err() {
        return;
    }
    let store = LevelStore::default_repo();
    let mut bad = Vec::new();
    for raw in list.split(',') {
        let id = raw.trim();
        if id.is_empty() {
            continue;
        }
        let id = id.strip_prefix('c').unwrap_or(id).replace('_', "-");
        let Ok(bundle) = store.load(&id) else {
            bad.push(format!("{}：读不到关卡文件", id));
            continue;
        };
        let Some(solution) = &bundle.solution else {
            bad.push(format!("{}：没有参考解", id));
            continue;
        };

        let runner = match crate::runner::Runner::new(bundle.config.clone(), solution) {
            Ok(r) => r,
            Err(errs) => {
                println!("【{}】参考解没编过：", id);
                for e in errs {
                    println!("    {}", e);
                }
                bad.push(id.clone());
                continue;
            }
        };

        // 逐条测例：直接调 runner，把期望与实际并排打出来
        let cases = match &bundle.config.goal {
            crate::config::Goal::Static { tests } | crate::config::Goal::Output { tests } => {
                tests.clone()
            }
            crate::config::Goal::Scripted { judge, .. } => {
                println!("【{}】scripted 判定（{}）—— 用 self_check 看结论", id, judge);
                bad.push(id.clone());
                continue;
            }
        };

        println!("\n【{}】{} 条测例（参考解）", id, cases.len());
        for (i, case) in cases.iter().enumerate() {
            let out = runner.run_case(case);
            let want = case.output_text.clone().unwrap_or_default();
            let got = out.state.output.clone();
            let ok = crate::judge::text_matches(&want, &got)
                && out.state.pause_kind != "error"
                && out.state.pause_kind != "limit";
            println!(
                "  {} #{} {}\n      输入 {}\n      期望 {}\n      实际 {}   [{} / {} 条指令]",
                if ok { "✓" } else { "✗" },
                i + 1,
                case.name,
                show(&case.input_text),
                show(&want),
                show(&got),
                out.state.pause_kind,
                out.state.instructions
            );
            if !ok {
                bad.push(format!("{} 的测例 #{}（{}）", id, i + 1, case.name));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "有问题的测例（逐条列出）：\n  {}",
        bad.join("\n  ")
    );
}

fn show(s: &str) -> String {
    format!("{:?}", s)
}

#[test]
fn every_level_page_and_test_config_are_consistent() {
    let store = LevelStore::default_repo();
    let ids = store.level_ids();
    assert!(!ids.is_empty(), "没有找到关卡");
    let mut problems = Vec::new();
    for id in &ids {
        match store.load(id) {
            Ok(bundle) => {
                // 关卡页与测例的 language 必须一致
                if bundle.source.editor.language != bundle.config.language {
                    problems.push(format!(
                        "{}：关卡页写的是 {:?}，测例写的是 {:?}",
                        id, bundle.source.editor.language, bundle.config.language
                    ));
                }
                if bundle.source.initial_code.trim().is_empty() {
                    problems.push(format!("{}：关卡页没有初始代码", id));
                }
                if bundle.source.editor.level != *id {
                    problems.push(format!(
                        "{}：关卡页里的 level 写的是 {}",
                        id, bundle.source.editor.level
                    ));
                }
                problems.extend(check_editor_constraints(&bundle));
            }
            Err(e) => problems.push(format!("{}：{}", id, e)),
        }
    }
    assert!(problems.is_empty(), "关卡文件不一致：\n{}", problems.join("\n"));
}

/// 检查编辑器约束的**行号是否有意义**。
///
/// 为什么需要这个：约束写错了**自检照样全绿**——因为自检只跑参考解，
/// 而参考解是直接喂给判定器的，根本不经过编辑器。真实事故：
/// 第四章有三关把 `readonlyPrefix` 写成了 `119`（本意大概是"只读前 119 行"），
/// 而初始代码只有 300+ 行；前端是"把前 N 行设成只读"，于是**每一行都被锁住**，
/// 玩家在编辑器里一个字都改不了 —— 关卡直接不可玩，但所有测试都是绿的。
///
/// 所以这里把"约束是否指到了真实存在的行、是否把所有行都锁死"变成一条测试。
fn check_editor_constraints(bundle: &crate::levels::LevelBundle) -> Vec<String> {
    let mut problems = Vec::new();
    let id = &bundle.id;
    let total = bundle.source.initial_code.lines().count();

    let c = bundle.source.editor.clone();
    for (name, n) in [
        ("readonlyPrefix", c.readonly_prefix),
        ("readonlySuffix", c.readonly_suffix),
    ] {
        if n > total {
            problems.push(format!(
                "{}：constraints.{} = {}，但初始代码只有 {} 行 —— \
                 前端会把它当成\"把前 {} 行设成只读\"，结果**整篇都锁住**，玩家一个字都改不了",
                id, name, n, total, n
            ));
        }
    }
    if c.readonly_prefix + c.readonly_suffix >= total && total > 0 {
        problems.push(format!(
            "{}：readonlyPrefix({}) + readonlySuffix({}) 覆盖了全部 {} 行，玩家没有任何可改的地方",
            id, c.readonly_prefix, c.readonly_suffix, total
        ));
    }
    for (which, lines) in [("hidden", &c.hidden), ("highlight", &c.highlight)] {
        for n in lines.iter() {
            if *n == 0 || *n > total {
                problems.push(format!(
                    "{}：constraints.{} 里的 {} 不在 1..={} 范围内（初始代码只有 {} 行）",
                    id, which, n, total, total
                ));
            }
        }
    }
    for (line, _) in c.suggested.iter() {
        if *line == 0 || *line > total {
            problems.push(format!(
                "{}：constraints.suggested 的键 {} 不在 1..={} 范围内（初始代码只有 {} 行）—— \
                 提示会指不到任何一行",
                id, line, total, total
            ));
        }
    }
    problems
}

/// 嵌入内容必须和磁盘上的内容一致。
///
/// 桌面壳（`app/src-tauri`）用嵌入内容跑游戏，所以"嵌入内容坏了"= 玩家拿到的是坏游戏。
/// 这个测试就是那道闸：它把两份内容逐关卡比一遍。
#[test]
#[cfg(feature = "embedded-content")]
fn embedded_content_matches_the_repository() {
    assert!(
        crate::embedded::count() > 0,
        "嵌入清单是空的 —— build.rs 没生成内容？"
    );

    let disk = LevelStore::default_repo();
    let embedded = LevelStore::embedded();

    let disk_ids = disk.level_ids();
    let emb_ids = embedded.level_ids();
    assert_eq!(disk_ids, emb_ids, "嵌入内容里的关卡列表和仓库里对不上");

    for id in &disk_ids {
        // 关卡页 + 测例 + 参考解都必须能读出来且一致
        let d = disk.load(id).unwrap_or_else(|e| panic!("磁盘上读不到 {}：{}", id, e));
        let e = embedded
            .load(id)
            .unwrap_or_else(|err| panic!("嵌入内容里读不到 {}：{}", id, err));
        assert_eq!(d.source.initial_code, e.source.initial_code, "{} 的初始代码不一致", id);
        assert_eq!(d.solution, e.solution, "{} 的参考解不一致", id);
        assert_eq!(d.source.editor.level, e.source.editor.level, "{} 的 level 不一致", id);
        assert_eq!(
            format!("{:?}", d.config.goal),
            format!("{:?}", e.config.goal),
            "{} 的判定配置不一致",
            id
        );
    }
}

#[test]
fn judge_registry_is_wired_up() {
    // 所有 judge 名字都要能在注册表里找到
    let store = LevelStore::default_repo();
    let known = crate::judge::scripted::names();
    for id in store.level_ids() {
        let Ok(cfg) = store.load_config(&id) else { continue };
        if let crate::config::Goal::Scripted { judge, .. } = &cfg.goal {
            assert!(
                known.contains(&judge.as_str()),
                "关卡 {} 用了未注册的 judge `{}`（可用的：{:?}）",
                id,
                judge,
                known
            );
        }
    }
}
