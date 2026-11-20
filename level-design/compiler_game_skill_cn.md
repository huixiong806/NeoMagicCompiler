# Compiler Game 技术栈 Skill（中文压缩版）

## 项目目标

一个“玩家用 C 语言逐步实现 C 编译器”的可编程游戏。

核心成长路径：

```text
学习汇编
→ 学习 C 语言
→ 完成表达式计算器
→ 完成简单 tiny C compiler
→ 加入多级IR，实现更复杂的目标代码生成策略
→ optimizer
```

本质上这是：

```text
Compiler Simulator + IDE + VM + Visualization Game
```

不是传统动作游戏。

---

# 核心架构

```text
Monaco Editor
    ↓
Compiler Frontend
    ↓
AST
    ↓
IR
    ↓
Bytecode
    ↓
VM / Simulation
    ↓
Game Events
    ↓
Godot Animation & Visualization
```

重要原则：

- Rendering 与 Simulation 解耦
- VM 控制逻辑
- Godot 只负责可视化状态变化

---

# 推荐技术栈

## 前端 / 游戏层

| 模块 | 技术 |
|---|---|
| 引擎 | Godot 4 |
| UI | Godot UI |
| 游戏脚本 | GDScript |
| 编辑器 | Monaco Editor |
| IDE 嵌入 | WebView |

原因：

- Godot UI 非常适合 IDE 风格布局
- GDScript 很适合 AI 辅助开发（vibe coding）
- Monaco 已解决编辑器复杂问题

---

## Compiler / Simulation Core

| 模块 | 技术 |
|---|---|
| Lexer | Rust |
| Parser | Rust |
| AST | Rust |
| IR | Rust |
| VM | Rust |
| Optimizer | Rust |
| Tick Simulation | Rust |

原因：

- Rust 非常适合 deterministic simulation
- 非常适合 compiler architecture
- enum/pattern matching 对 AST 很舒服
- VM 性能优秀
- 后续易扩展 WASM/plugin system

---

# Godot + Rust 集成

使用：

```text
Godot GDExtension
```

架构：

```text
Godot (frontend)
    ↓
Rust dynamic library
    ↓
Compiler + VM + Simulation
```

Godot 负责：

- UI
- 动画
- 特效
- debugger visualization
- 游戏流程

Rust 负责：

- parsing
- bytecode
- execution
- optimization
- deterministic simulation

---

# Monaco 集成

Monaco 本质：

```text
HTML + JS + CSS
```

通过 WebView 嵌入 Godot。

通信结构：

```text
Monaco ↔ JS Bridge ↔ Godot ↔ Rust VM
```

Monaco 不应该直接控制游戏。

正确结构：

```text
Player Code
→ Compiler
→ VM
→ Events
→ Godot Playback
```

---

# 版权与商业化

## Godot

License：

```text
MIT
```

允许：

- 闭源商业化
- 修改引擎
- Steam 发布
- 专有逻辑

仅需：

- 保留 Godot copyright notice

---

## Monaco / VSCode

推荐：

- Monaco Editor
- Code-OSS 思路

避免：

- Microsoft branding
- 重新分发官方 VSCode binary

安全可做：

- 商业化 IDE
- 自定义 UI
- 插件系统
- 魔改编辑器

---

# Simulation 设计

游戏应该是：

```text
tick-based deterministic simulation
```

而不是 frame-driven logic。

例：

```text
Tick 100:
- robot move
- memory update
- instruction execute
```

Rendering 只负责播放。

---

# VM 设计

不要直接解释执行 AST。

正确流程：

```text
Source
→ Lexer
→ Parser
→ AST
→ IR
→ Bytecode
→ VM
```

VM 示例：

```rust
enum OpCode {
    Move,
    Add,
    Jump,
    Load,
    Store,
}
```

---

# 推荐开发阶段

## Phase 1（MVP）

只做：

- Monaco editor
- single robot
- tiny asm
- VM execution
- move animation
- step debugger

不要做插件系统。

---

## Phase 2

加入：

- memory viewer
- register visualization
- timeline
- AST visualizer
- IR visualizer

---

## Phase 3

加入：

- tiny C compiler
- self-hosting progression
- optimizer
- WASM plugins
- community levels

---

# 重要的范围控制

不要：

- 自己写文本编辑器
- 自己写 rendering engine
- 一开始实现完整 C
- 上来就 LLVM/JIT
- 模拟真实 x86

重点：

```text
Understandable systems
```

而不是：

```text
Perfect realism
```

---

# 推荐参考项目

灵感：

- Turing Complete
- TIS-100
- SHENZHEN I/O
- Human Resource Machine
- Factorio

Compiler 参考：

- chibicc
- 8cc
- TinyCC

---

# 最终推荐技术栈

```text
Godot 4
+ GDScript
+ Monaco Editor
+ Rust Compiler Core
+ GDExtension
+ Tick-based VM Simulation
```
