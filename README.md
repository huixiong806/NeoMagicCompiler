# Neo Magic Compiler（`no_story` 分支）

一个面向零基础玩家的**强游戏化交互编译原理教学游戏**：从「手动当 CPU」认识寄存器与 RISC-V 汇编出发，经过 C 语言与编译器开发基础、极简栈式绘图机翻译器实践，最终在本地用真正的 C 语言维护并持续迭代一个将 C 语言编译到 RISC-V 汇编的完整编译器。

> **分支说明（`no_story`）**：本分支暂时移除所有剧情与世界观包装，专注于**强游戏化交互体验、零基础教学闭环与网页化工程架构**的构建。后续剧情可作为独立叙事层叠加，不耦合核心玩法。

---

## 核心设计理念

1. **强游戏化交互：先手玩，再写代码**：告别大段纯文字教程与被动看图。围绕 **4 套可复用的可玩机制沙盘**（「手动当 CPU」桌面、内存与指针画布、切词传送带 & AST 积木台、实时产线与可视化终端），让玩家先亲手操作建立肌肉记忆，再写代码把手工流程自动化。
2. **平滑的难度曲线与多章渐进拆分**：单关只引入一个新概念，配套渐进脚手架与 3 级按需提示；第 2 章系统教授写编译器所需的 C 语言工具，第 3 章先用极简栈式绘图机（`DrawVM`）跑通编译前端与代码生成，第 4~6 章按「纯核心 → 功能扩展 → 高级扩展」逐步升级。
3. **纯网页交互 + 本地真实工程联动**：放弃 `Tauri + mdBook`，全面转向现代 Web 应用（`React + Tailwind + CodeMirror 6`），同时支持**线上静态访问**与 **`nmc-bridge` 本地一体托管**。前两章通过 **WebAssembly** 零安装运行，同时全章节均支持通过 **`nmc-bridge`** 连接本地 IDE（第 1~2 章支持单文件监听，第 3~6 章支持完整 C 工程自动构建与测试）。
4. **独立的通用 Playground**：提供独立的 `/playground` 页面（可自由沙盒实验，也可挂载任意关卡测例集），统一承载在线编辑、本地单文件与本地 C 工程三种工作流。

---

## 六章学习路线

| 章 | 主题 | 运行与开发模式 | 玩家做什么 | 核心定位 |
|----|------|--------------|-----------|---------|
| 第一章 | RISC-V 汇编入门（RV32IM） | 纯网页 WASM / 可选本地单文件 | 先「手动当 CPU」再写 RISC-V 汇编 | 建立寄存器、内存、分支、循环、栈与函数调用的底层直觉 |
| 第二章 | C 语言与编译器开发基础 | 纯网页 WASM / 可选本地单文件 | 编写 C 程序并对照内存画布与汇编 | **范围大于第 4 章源语言**：系统掌握指针、字符串、结构体、递归、数组/树/符号表等写编译器必备技能 |
| 第三章 | 迷你编译器：绘图脚本 → `DrawVM` | **本地 C 项目 + 网页 Playground** | 用本地 C 工程把绘图脚本编译成 12 指令栈式绘图机码，实时在网页画出图案 | 零压力打通本地开发联动，用极简栈式机完整走通「词法 → 语法树 → 代码生成」 |
| 第四章 | 核心编译器：C 核心子集 → RISC-V | **本地 C 项目 + 网页 Playground** | 持续迭代本地 C 工程，将纯核心 C（`int`/表达式/变量/分支循环/函数递归）编译成单文件 RISC-V 汇编 | 跑通端到端真机运行，亲手造出第一个真实编译器 |
| 第五章 | 编译器功能扩展 | **本地 C 项目 + 网页 Playground** | 在第 4 章工程上继续扩展：数组、指针、字符串、`struct`、位运算、多文件/链接等 | 从“教学核心编译器”进化为实用子集编译器 |
| 第六章 | 高级扩展（暂不定） | **本地 C 项目 + 网页 Playground** | 预留扩展方向：多级 IR、编译优化与周期评分榜，或更多高级语言特性 | 开放式终局拓展 |

---

## 整体架构概览

```text
浏览器（Web SPA：React + Tailwind + CodeMirror 6｜支持线上静态托管 & 本地托管）
  ├─ 关卡地图 / 4 套强游戏化交互沙盘 / 独立 Playground / 概念图鉴 Wiki
  ├── [模式 A：在线引擎]  crates/nmc-wasm（WebAssembly，零安装运行第 1~2 章与在线沙盒）
  └── [模式 B：本地联动]  HTTP + WebSocket (127.0.0.1:17320)
                              │
                              ▼
                     crates/nmc-bridge（本地轻量桥接服务 + 可选静态资源托管）
                      ├─ 启动环境体检（检测本机 gcc/clang/make 并提供清晰指引）
                      ├─ 支持第 1~2 章本地单文件监听 & 第 3~6 章 C 工程识别（nmc.project.json）
                      ├─ 调用本机编译器构建玩家工程，沙箱超时运行并捕获产物
                      └─ 交给核心引擎验证（DrawVM / RISC-V 模拟器），推送可视化轨迹给网页
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
├── web/                         # 现代 Web 前端（Vite + TypeScript + React + Tailwind + CodeMirror 6）
│   ├── src/pages/               #   关卡地图 / 关卡学习页 / 独立 Playground / Wiki 图鉴
│   ├── src/sandboxes/           #   4 套强游戏化交互沙盘（手动 CPU / 内存画布 / 词法AST台 / 产线与DrawVM）
│   └── src/engine/              #   统一执行适配器（自动切换 WASM 引擎 与 nmc-bridge）
├── content/                     # 关卡与教学内容（从原 compiler-content/ 迁移演进）
│   ├── levels/cN_M/             #   单关目录：meta.json + lesson.md + tests.json + solution.*
│   ├── templates/               #   第 3~5 章本地 C 项目脚手架模板（含 nmc.project.json）
│   └── wiki/                    #   按需查阅的交互式概念图鉴与参考算法卡片
├── crates/
│   ├── riscv-simulator/         # [已有复用] 完整 RV32IM 模拟器 + MMIO + 历史回溯
│   ├── riscv-assembler/         # [已有复用] 两遍扫描汇编器（伪指令、指示符、白名单）
│   ├── starlang/                # [已有复用] C 子集 → RISC-V 汇编编译器
│   ├── neomagic-core/           # [已有复用] 关卡加载 / 运行器 / 判定器 / DrawVM / 存档 / 自检
│   ├── nmc-wasm/                # [规划新增] 面向浏览器的 WASM 绑定层
│   └── nmc-bridge/              # [规划新增] 本地项目桥接服务（体检/监听/构建/测试/调试/托管）
├── documents/                   # 设计与开发文档（详见下文索引）
│   └── archive/                 #   早期历史文档（仅作归档参考）
├── Makefile                     # 统一构建与自检入口
└── README.md
```

> **关于原 `app/`（Tauri）与 `compiler-content/`（mdBook）**：现有 Rust 核心引擎与第 1~2 章原型测例/参考解完整保留作为迁移基线；在网页版闭环跑通前暂不删除旧目录，待验证完成后再清理归档。

---

## 快速开始

### 环境要求

- **基础开发（前端 + 核心引擎）**：Node.js ≥ 18、Rust ≥ 1.75、`wasm-pack`（构建 WASM 引擎）。
- **本地 C 项目联动（第 3~6 章）**：本机 C 工具链（`gcc` 或 `clang`，Windows 推荐 MSYS2 MinGW-w64；`nmc-bridge` 启动时会自动体检并给出配置指引）。

### 常用命令

```bash
# 1. 运行核心引擎测试与关卡自检（内容质量底线）
cargo test --workspace
cargo test -p neomagic-core selfcheck -- --nocapture

# 2. 只自检指定关卡
NMC_SELFCHECK=1-1,1-2 cargo test -p neomagic-core selfcheck_only -- --nocapture

# 3. 启动网页前端（Phase 1 落地后生效）
cd web && npm install && npm run dev

# 4. 启动本地桥接服务并绑定本地项目或单文件（Phase 3 落地后生效）
cargo run -p nmc-bridge -- --project ./my-compiler
```

> 仓库根的 `.cargo/config.toml` 配置了 `[net] offline = true` 以避免多进程构建争抢索引锁；若首次拉取新增 crate 依赖，可临时联网缓存后再恢复离线模式。

---

## 关卡自检：不可妥协的质量底线

本项目保留并强化**引擎关卡自检**机制：用每一关的参考解跑通该关全部测例。

- 自检通过 → 关卡确定有解，测例与参考解自洽；
- 编辑器约束与配置校验通过 → 不会出现锁死编辑区或提示行号越界等“测试绿但不可玩”的问题。

第 3~5 章的本地 C 项目关卡同样遵循自检原则：由参考 C 编译器项目编译并执行端到端判定，确保每关设计真实可解。

---

## 文档导航

| 文档 | 职责与适用场景 |
|------|---------------|
| [`documents/目标与形态.md`](documents/目标与形态.md) | **游戏总规划与闭环设计**：4 套强游戏化交互沙盘、零基础闭环、难度曲线、本地项目联动体验、1~6 章定位与路线图 |
| [`documents/总设计.md`](documents/总设计.md) | **核心规格与六章大纲**：RV32IM ISA、C 语言规格、第 3 章 `DrawVM` 栈式绘图机规格、各章详细大纲与 Judge 体系 |
| [`documents/架构.md`](documents/架构.md) | **网页化与本地桥接技术架构**：React 前端栈、双部署模式、Rust 核心复用、`nmc-wasm`、`nmc-bridge` 协议与独立 Playground |
| [`documents/关卡编写规范.md`](documents/关卡编写规范.md) | **关卡编写指南**：无剧情版关卡目录、4 套沙盘配置、分级提示、在线/本地单文件/本地工程三模式测例、RISC-V 避坑清单 |
| [`documents/存档格式.md`](documents/存档格式.md) | **存档与同步契约**：浏览器端存储、导入导出、`nmc-bridge` 本地无损同步与项目状态记录 |
| [`documents/开发流程.md`](documents/开发流程.md) | **开发与验证流程**：环境配置、构建测试、关卡自检、Windows 编码与提交规范 |
| [`documents/术语表.md`](documents/术语表.md) | **统一术语与命名约定**：跨文档核心名词、模块标识符与历史归档清单 |
| [`documents/交接文档.md`](documents/交接文档.md) | **项目交接与推进清单**：当前资产盘点、`no_story` 已定决策、下一步具体执行项 |
