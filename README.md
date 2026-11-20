# Neo Magic Compiler（`no_story` 分支）

一个面向零基础玩家的**交互式编译原理教学游戏**：从认识寄存器与 RISC-V 汇编出发，经过 C 语言与核心数据结构训练、迷你指令翻译器实践，最终在本地用真正的 C 语言维护并完成一个将 C 子集编译到 RISC-V 汇编的完整编译器。

> **分支说明（`no_story`）**：本分支暂时移除所有剧情与世界观包装，专注于**游戏核心闭环、零基础可视化教学体验与网页化工程架构**的构建。后续剧情可作为独立叙事层叠加，不耦合核心玩法。

---

## 核心设计理念

1. **零基础友好，靠图像和交互动画学习**：告别大段纯文字教程。每一关通过交互式动画（寄存器数据流动、内存格子、栈帧压弹、词法扫描传送带、AST 生长、小机器人执行）建立直觉，再动手写代码。
2. **平滑的难度曲线与持续正面反馈**：单关只引入一个新概念，配套渐进式脚手架与分级提示；每次运行提供可视化状态变化与失败测例一键回放调试，通关即时点亮技能树与编译器流水线节点。
3. **纯网页交互 + 本地真实工程联动**：放弃 `Tauri + mdBook` 方案，改用现代 Web 应用承载教学与可视化；前两章通过 **WebAssembly** 在浏览器零安装运行，后两章通过轻量本地桥接服务 **`nmc-bridge`** 自动识别本地 C 语言项目，实现**本地 IDE 写代码 + 网页端一键构建、测试、可视化调试**。
4. **独立的通用 Playground**：提供独立的 Playground 页面（可自由沙盒实验，也可挂载任意关卡测例集），同时支持「在线编辑模式」与「本地项目模式」，贯穿全流程。

---

## 四章学习路线

| 章 | 主题 | 运行模式 | 玩家做什么 | 核心收获 |
|----|------|----------|-----------|---------|
| 第一章 | RISC-V 汇编入门（RV32IM） | 纯网页（WASM） | 编写 RISC-V 汇编指令 | 建立 CPU、寄存器、内存、分支、循环、栈与函数调用的底层直觉 |
| 第二章 | C 语言与编译器前置基础 | 纯网页（WASM）/ 可选本地 | 编写 C 子集程序（可实时对照生成的汇编） | 掌握指针、字符串、结构体、递归、数组/树/符号表等写编译器必备技能 |
| 第三章 | 迷你翻译器（装置脚本 → 装置指令） | **本地 C 项目 + 网页 Playground** | 用真实 C 语言实现小型脚本翻译器，驱动网页中的虚拟装置动画 | 零压力走通本地开发联动，首次完整体验「词法 → 语法 → 指令生成」 |
| 第四章 | 真正的编译器（C 子集 → RISC-V 汇编） | **本地 C 项目 + 网页 Playground** | 持续迭代同一个本地 C 工程，逐关完成编译器各阶段并通过端到端运行验证 | 亲手产出能编译并运行真实程序的编译器 |

第五章（多级 IR、优化 Pass 与性能评分）保留架构扩展能力，暂不在当前范围。

---

## 整体架构概览

```text
浏览器（Web SPA：关卡地图 / 交互动画教学 / 独立 Playground / 概念图鉴 Wiki）
  ├── [模式 A：在线引擎]  crates/nmc-wasm（WebAssembly，零安装运行第 1~2 章与在线沙盒）
  └── [模式 B：本地联动]  HTTP + WebSocket (127.0.0.1:17320)
                              │
                              ▼
                     crates/nmc-bridge（本地轻量桥接服务）
                      ├─ 识别本地 C 项目（nmc.project.json）与文件变动监听
                      ├─ 调用本地 gcc / clang / make 构建玩家编译器
                      ├─ 运行玩家编译器并捕获输出的 RISC-V 汇编（含超时/崩溃保护）
                      └─ 交给核心引擎汇编、模拟执行与判定，推送可视化轨迹给网页
                              │
                              ▼
                     crates/neomagic-core（复用现有领域层）
                      ├─ crates/riscv-assembler（两遍扫描汇编器）
                      ├─ crates/starlang（C 子集参考编译器）
                      └─ crates/riscv-simulator（RV32IM 模拟器 + 历史回溯）
```

---

## 规划中的项目结构

```text
├── web/                         # 现代 Web 前端（Vite + TypeScript + React/Tailwind）
│   ├── src/pages/               #   关卡地图 / 关卡学习页 / 独立 Playground / Wiki 图鉴
│   ├── src/visualizers/         #   交互式动画组件（CPU/内存/栈/词法/AST/迷你装置）
│   └── src/engine/              #   统一执行适配器（自动切换 WASM 引擎 与 nmc-bridge）
├── content/                     # 关卡与教学内容（从原 compiler-content/ 迁移演进）
│   ├── levels/cN_M/             #   单关目录：meta.json + lesson.md + tests.json + solution.*
│   ├── templates/               #   第 3~4 章本地 C 项目脚手架模板（含 nmc.project.json）
│   └── wiki/                    #   按需查阅的交互式概念图鉴与参考算法卡片
├── crates/
│   ├── riscv-simulator/         # [已有复用] 完整 RV32IM 模拟器 + MMIO + 历史回溯
│   ├── riscv-assembler/         # [已有复用] 两遍扫描汇编器（伪指令、指示符、白名单）
│   ├── starlang/                # [已有复用] C 子集 → RISC-V 汇编编译器
│   ├── neomagic-core/           # [已有复用] 关卡加载 / 运行器 / 判定器 / 存档 / 自检
│   ├── nmc-wasm/                # [规划新增] 面向浏览器的 WASM 绑定层
│   └── nmc-bridge/              # [规划新增] 本地项目桥接守护进程（构建/监听/测试/调试）
├── documents/                   # 设计与开发文档（详见下文索引）
│   └── archive/                 #   早期历史文档（仅作归档参考）
├── Makefile                     # 统一构建与自检入口
└── README.md
```

> **关于原 `app/`（Tauri）与 `compiler-content/`（mdBook）**：现有 Rust 核心引擎与 53 关原型测例/参考解完整保留作为迁移基线；在网页版闭环跑通前暂不删除旧目录，待验证完成后再清理归档。

---

## 快速开始

### 环境要求

- **基础开发（前端 + 核心引擎）**：Node.js ≥ 18、Rust ≥ 1.75、`wasm-pack`（构建 WASM 引擎）。
- **本地 C 项目联动（第 3~4 章）**：本机 C 工具链（`gcc` 或 `clang`，Windows 推荐 MSYS2 MinGW-w64）。

### 常用命令

```bash
# 1. 运行核心引擎测试与关卡自检（内容质量底线）
cargo test --workspace
cargo test -p neomagic-core selfcheck -- --nocapture

# 2. 只自检指定关卡
NMC_SELFCHECK=1-1,1-2 cargo test -p neomagic-core selfcheck_only -- --nocapture

# 3. 启动网页前端（规划中，迁移完成后生效）
cd web && npm install && npm run dev

# 4. 启动本地桥接服务并绑定本地 C 项目（规划中，第 3~4 章使用）
cargo run -p nmc-bridge -- --project ./my-compiler
```

> 仓库根的 `.cargo/config.toml` 配置了 `[net] offline = true` 以避免多进程构建争抢索引锁；若首次拉取新增 crate 依赖，可临时传 `--offline=false` 或更新本地缓存后再恢复。

---

## 关卡自检：不可妥协的质量底线

本项目保留并强化**引擎关卡自检**机制：用每一关的参考解跑通该关全部测例。

- 自检通过 → 关卡确定有解，测例与参考解自洽；
- 编辑器约束与配置校验通过 → 不会出现锁死编辑区或提示行号越界等“测试绿但不可玩”的问题。

第 3~4 章的本地 C 项目关卡同样遵循自检原则：由参考 C 编译器项目编译并执行端到端判定，确保每关设计真实可解。

---

## 文档导航

| 文档 | 职责与适用场景 |
|------|---------------|
| [`documents/目标与形态.md`](documents/目标与形态.md) | **游戏总规划与闭环设计**：零基础教学体系、交互式动画规划、难度曲线、正面反馈、本地项目联动体验、分阶段路线图 |
| [`documents/总设计.md`](documents/总设计.md) | **核心规格与四章大纲**：RV32IM ISA、C 子集规格、第 3 章迷你装置指令集、四章关卡大纲与 Judge 体系 |
| [`documents/架构.md`](documents/架构.md) | **网页化与本地桥接技术架构**：Rust 核心复用、WASM 引擎、`nmc-bridge` 协议、`nmc.project.json`、独立 Playground 架构 |
| [`documents/关卡编写规范.md`](documents/关卡编写规范.md) | **关卡编写指南**：无剧情版关卡结构、交互动画配置、分级提示、在线/本地双模式测例、RISC-V 避坑清单 |
| [`documents/存档格式.md`](documents/存档格式.md) | **存档与同步契约**：浏览器端存储、导入导出、`nmc-bridge` 本地同步与项目状态记录 |
| [`documents/开发流程.md`](documents/开发流程.md) | **开发与验证流程**：环境配置、构建测试、关卡自检、Windows 编码与提交规范 |
| [`documents/交接文档.md`](documents/交接文档.md) | **项目交接与推进清单**：当前资产盘点、`no_story` 改造决策、下一步具体执行项 |
