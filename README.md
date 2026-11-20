# Neo Magic Compiler

一个**编译原理教学游戏**：玩家从学习RISCV汇编开始，一路走到自己能写出一个把C子集语言编译成RISCV的编译器游戏。

四章共 53 关，全部带测例与参考解；引擎会用参考解逐关自检，
保证"每一关都确实可解"（见下文《关卡自检》）。

---

## 世界观与术语

本分支无剧情

## 四章内容

| 章 | 主题 | 关数 | 玩家写什么 |
|----|------|:---:|-----------|
| 第一章 | 星之语入门（RV32IM） | 15 | 星之语汇编 |
| 第二章 | 逐渐抽象的魔法 | 14 | 星咒（C 子集） |
| 第三章 | 简单的魔法翻译机 | 12 | 星之语程序：读字符流 → 扫描 → 求值 |
| 第四章 | 魔导符的编织 | 12 | **写编译器**：星咒源码 → 星之语汇编 |

第五章（多级 IR 与编译优化）只保留扩展能力，不在当前范围。

## 项目结构

```
├── compiler-content/            # 教学内容（mdBook 书 + 关卡三件套）
│   ├── src/cN_M.md              #   关卡页：叙事 + ```riscv 块（编辑器配置 + 初始代码）
│   ├── src/tests/cN_M.json      #   测例与判定配置
│   ├── solutions/cN_M.s|.c      #   参考解（自检用它验证关卡可解）
│   ├── frontend/                #   编辑器前端（TypeScript + Lit + Vite）
│   └── book.toml                #   输出到 app/src/
├── app/                         # Tauri 2 桌面应用
│   ├── src/                     #   mdBook 构建产物（生成物，勿手改）
│   └── src-tauri/               #   IPC 层（api.rs 逻辑 / dto.rs 接口 / lib.rs 转发）
├── crates/
│   ├── riscv-simulator/         #   完整 RV32IM 模拟器 + MMIO + 历史回溯
│   ├── riscv-assembler/         #   两遍扫描汇编器（伪指令、指示符、白名单）
│   ├── starlang/                #   星咒 → 星之语 编译器
│   └── neomagic-core/           #   领域层：关卡加载 / 运行器 / 判定器 / 存档 / 自检
├── documents/                   # 设计文档（总设计 / 架构 / 关卡编写规范 / 开发流程 / 交接文档 / 术语表 / 存档格式）
│   └── archive/                 #   早期设计文档（已被取代，只作历史参考）
├── Makefile                     # 构建入口
└── README.md
```

## 快速开始

环境要求：**Node.js ≥ 18**、**Rust ≥ 1.75**、**mdBook ≥ 0.4**（`cargo install mdbook`）。

```bash
make smoke            # 环境自检：cargo / node / mdbook 都在吗
make setup            # 安装 app/ 与 frontend/ 的 npm 依赖
make dev              # 构建书 + 启动桌面版（推荐：玩游戏的完整形态）
make book-serve       # 只预览教学内容（浏览器热更新，http://localhost:3000）
make book-build       # 只构建书（前端 TS 编译 + mdBook）
make test             # 关卡自检 + cargo test --workspace
make book-test        # 只跑关卡自检（内容的守门人）
make theme-check      # TypeScript 类型检查
make check            # cargo check --workspace
make release          # 打包桌面安装包
```

> **为什么不用 `mdbook test`**：它只会把**没标语言**的代码块当 Rust 去编译，
> 而关卡页里的块是星之语汇编、token 序列、伪代码这类自定义文本，
> 于是几十处都会报 `cannot find value`／`undeclared type` —— 那些报错没有意义。
> 本项目内容层的守门人是**引擎自己的关卡自检**（见下节），
> 它比"Rust 代码块能不能编译"有用得多。

> 浏览器预览（`book-serve`）里编辑器可以写代码，但**运行 / 调试 / 判定需要在桌面版里**
> （那些能力在 Rust 后端）。前端会明确提示这一点，不会静默失败。

## 关卡自检：这个项目的质量底线

```bash
cargo test -p neomagic-core selfcheck -- --nocapture
```

它拿**每一关的参考解**去跑该关的全部测例。含义很直接：

- 自检通过 → 这一关**至少有解**，测例与参考解自洽；
- 自检失败 → 说明测例写错了或参考解写错了，**关卡不可解**，必须修。

只检查某几关（写关卡时用）：

```bash
# Windows PowerShell
$env:NMC_SELFCHECK="3-4,3-5,3-6"; cargo test -p neomagic-core selfcheck_only -- --nocapture
# bash
NMC_SELFCHECK=3-4,3-5,3-6 cargo test -p neomagic-core selfcheck_only -- --nocapture
```

> 仓库根的 `.cargo/config.toml` 设了 `[net] offline = true`：依赖全在本地缓存，
> 构建不需要访问 crates.io。多个进程同时"更新索引"会全部堵在包缓存锁上，
> 表现得像死锁（一堆 cargo、零个 rustc）—— 这个坑已经踩过一次。

## 架构概要

```
mdBook HTML（关卡页，构建产物在 app/src/）
  │  页面里注入 theme/dist/riscv-playground.js（Lit 组件）
  │  window.__TAURI__.core.invoke(...)   ← 15 条 IPC 命令
  ▼
app/src-tauri/        Tauri 2 桌面壳
  ├─ lib.rs           IPC 层：只做参数转发与错误包装（"薄"到每行只有一次调用）
  ├─ api.rs           逻辑层：不依赖 Tauri，所以能被 cargo test 直接覆盖
  └─ dto.rs           接口定义：与 frontend/src/core/types.ts 一一对应（全 camelCase）
  ▼
neomagic-core         领域层：关卡加载 / 运行 / 判定 / 存档 / 自检
  ├─ riscv-assembler  星之语 → 机器码（两遍扫描）
  ├─ starlang         星咒 → 星之语文本
  └─ riscv-simulator  机器码 → 真的跑起来（MMIO / 系统调用 / 历史回溯）
```

关卡内容在**打包时嵌入二进制**（`crates/neomagic-core/build.rs` 生成清单，
`embedded-content` 特性**默认开启**），所以装好的游戏不依赖仓库目录；
开发时优先读磁盘，改完关卡重新构建就能看到。细节见
[`documents/架构.md`](documents/架构.md)。

## 文档入口

| 文档 | 什么时候读 |
|------|-----------|
| [`documents/总设计.md`](documents/总设计.md) | **冻结的设计与大纲**：世界观、四章大纲、ISA、星咒规格、关卡 schema 的设计意图 |
| [`documents/架构.md`](documents/架构.md) | **现在的实现**：五个 crate 的职责与边界、桌面壳三层分工、15 条 IPC 命令表、数据流、内容管线、两门验证机制、历史回溯设计、已发现的文档/实现差异 |
| [`documents/关卡编写规范.md`](documents/关卡编写规范.md) | **写关卡**：三件套格式、测例 JSON 全字段、可用指令、以及一堆踩过的坑 |
| [`documents/开发流程.md`](documents/开发流程.md) | **照着做就能跑起来**：环境要求、构建、测试、自检、写新关卡、提交规范、Windows 编码坑 |
| [`documents/交接文档.md`](documents/交接文档.md) | **接手开发先读这份**：当前进度、下一步、常用命令、给子代理写简报的模板 |
| [`documents/术语表.md`](documents/术语表.md) | 名词与标识符命名约定（跨文档对齐用） |
| [`documents/存档格式.md`](documents/存档格式.md) | 要动 `user_progress.json` 或编辑器草稿格式时（跨运行的活契约） |

早期设计文档（需求分析、软件架构、接口定义、需求进度、模块设计、关卡设计师指南）
已被上面这几份取代，`git mv` 收进 [`documents/archive/`](documents/archive/README.md) 只作历史参考；
取代关系与"哪些方案已废弃"见那份 README。

---

## 给接手开发的 AI 的提示（项目发起人原始要求，保留备查）

游戏应该包含下列内容：

第一章 RiscV汇编语言教学关卡

第二章 C语言/必要算法教学关卡

第三章 从零开始用C语言构建一个“sub compiler”，例如表达式计算器（但自己创新一下，不要真的用表达式计算器，太没创新了）

第四章 从零开始用C语言构建真正的编译器，词法分析->语法分析->AST生成->汇编生成，只追求功能正确，不追求性能。

第五章（暂时不需要开发，只是保留这扩展能力）多级IR以及编译优化。

其他要求：

1. 先像专业的游戏策划师一样，做设计，再开始实现！！！要真正站在玩家角度考虑，做出可以理解并有趣的游戏。否则真的动工之后再想大改架构将会特别麻烦。关卡内容和顺序安排你自己决定，尤其是最后开发编译器使用的语言的知识范围，没教过的用法/算法不能莫名其妙出现。
2. 包含完整顺畅的剧情，第四章结束剧情就可以收尾了，第五章开启新篇章。
3. 做长期可维护的代码，不要可读性过差或者局部复杂性过高导致不可维护，注释用中文。
4. 我们的项目如果没那么复杂，也不用浪费太多时间维护文档，当然你觉得文档有必要的话，最好还是有。
5. git可以提交，不要提交敏感信息，不要破坏仓库。新开一个分支完成一切，不要对主分支做任何变动。
6. 现在已有的内容还很简陋，不喜欢或者不能满足要求也可以推翻。

> 落实说明：第三章与第四章的"编译器"是**用星之语（汇编）写的**，
> 而不是原始要求里的"C 语言"。理由：第四章的产物必须能被真正汇编执行，
> 而星之语是本项目唯一有完整工具链的目标语言；玩家用星之语写编译器，
> 也正好把第一、二章学的两门语言都用上。第三章则先用星之语把
> "读字符流 → 扫描 → 求值"这条路走通，为第四章铺路。
