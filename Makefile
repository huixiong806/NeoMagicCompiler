# Neo Magic Compiler — 根目录构建入口
#
# 需要：GNU make、mdbook、Node.js、Rust 工具链。
#
# 常用命令速查:
#   make dev          本地运行 Tauri 桌面开发版（构建书 + 启动桌面版）
#   make book-serve   仅 mdbook，浏览器预览（热更新，http://localhost:3000）
#   make book-build   只构建书（前端 TS 编译 + mdBook）
#   make book-test    关卡自检（内容的守门人）
#   make test         关卡自检 + cargo test --workspace
#   make theme-check  TypeScript 类型检查
#   make check        cargo check --workspace
#   make release      打包完整 Tauri 安装包

# ⚠ Windows 上的坑（踩过两次，记在这里免得后人再查）：
#
# `make` 默认会用 PATH 里的 sh/bash 来跑每条命令（本机是 MSYS2 的 sh）。
# 而 Windows 的 npm 只提供 `npx.cmd` / `npm.cmd`，**POSIX shell 不会自动补 `.cmd`**，
# 于是报
#     /bin/bash: C:/Program Files/nodejs/npx: No such file or directory
# 看起来像"Node 没装"，其实是 shell 找不到带扩展名的那个文件。
#
# 试过 `SHELL := pwsh` + `.SHELLFLAGS`，但 MinGW 的 make 会照旧拿 sh 去跑
# （表现为 `/usr/bin/sh: -N: invalid option`），所以不折腾 shell 设置了，改用两条规则：
#   1. 调 npm/npx 一律写 `.cmd` 后缀；
#   2. **能绕过 npm 就绕过** —— 跑 Tauri 直接调 `./node_modules/.bin/tauri.cmd`，
#      不经 `npm run`（`npm run` 内部还会再去找无扩展名的 `npm`，又踩回同一个坑）。
#
# 另外两条经验：
#   - `@echo` 只在 sh 里是命令，pwsh 里不是；这里统一按 sh 语义写。
#   - `echo` 的内容**不能有中文**：中文 Windows 下 sh 会按 GBK 解释 makefile，
#     带中文的 echo 会变成乱码命令名报 "command not found"（注释不受影响）。
#   - 若终端出现「无法分析 AutoRun 注册表项中指定的值」，那是本机 cmd.exe 的
#     AutoRun 项坏了（与本项目无关），只是噪音，不影响构建结果。

CONTENT_DIR := compiler-content
APP_DIR     := app

.PHONY: help setup smoke \
	book-build book-test book-serve \
	theme-build theme-check \
	check test-sim test \
	dev release clean

.DEFAULT_GOAL := help

# 开工前的环境自检：缺什么就让 shell 直接报出来，别让人对着 127 猜。
#
# ⚠ 这里刻意**只用 ASCII + 不做花哨的复合命令**，两个原因都踩过：
#   1. MSYS2 的 sh 对 `which a b c >/dev/null 2>&1 || {...}` 会直接 127；
#   2. 中文 Windows 下 sh 会按 **GBK** 解释 makefile —— 带中文的 echo 会变成乱码命令名，
#      报 "command not found"（写中文注释没问题，注释不进 shell，但 echo 的内容会）。
smoke:
	@cargo --version
	@node --version
	@mdbook --version
	@test -f $(APP_DIR)/node_modules/.bin/tauri.cmd || { echo "MISSING: app/node_modules (run: make setup)"; exit 1; }
	@echo "OK: cargo / node / mdbook / tauri-cli found. Next: make dev"

help:
	@echo "Neo Magic Compiler - available targets:"
	@echo ""
	@echo "  mdbook:"
	@echo "    make book-build     build the book only (frontend bundle + mdbook)"
	@echo "    make book-test      level self-check: every level must be solvable"
	@echo "    make book-serve     preview in browser (http://localhost:3000)"
	@echo ""
	@echo "  theme (editor frontend):"
	@echo "    make theme-build    build Lit/TS playground bundle"
	@echo "    make theme-check    TypeScript type check only"
	@echo ""
	@echo "  Rust:"
	@echo "    make check          cargo check --workspace"
	@echo "    make test-sim       cargo test -p riscv-simulator (fast)"
	@echo ""
	@echo "  full project:"
	@echo "    make test           level self-check + cargo test --workspace"
	@echo "    make dev            build the book, then run the Tauri desktop app"
	@echo "    make release        build the book, then package the installer"
	@echo ""
	@echo "  other:"
	@echo "    make smoke          check that the required tools are installed"
	@echo "    make setup          npm install (app + frontend)"	@echo "    make clean          remove generated files"

setup:
	cd $(APP_DIR) && npm.cmd install
	cd $(CONTENT_DIR)/frontend && npm.cmd install

theme-build:
	cd $(CONTENT_DIR)/frontend && npx.cmd vite build

theme-check:
	cd $(CONTENT_DIR)/frontend && npx.cmd tsc --noEmit

book-build: theme-build
	cd $(CONTENT_DIR) && mdbook build

# ⚠ 这里**故意不用 `mdbook test`**：它只会把没标语言的代码块当 **Rust** 去编译，
# 而本项目关卡页里的代码块是星之语汇编、token 序列、伪代码这类自定义文本，
# 于是几十处都会报 "cannot find value / undeclared type" —— 那些报错没有任何意义。
#
# 关卡内容真正的守门人是引擎自己的"关卡自检"：
#   用每一关的参考解跑该关的全部测例，并且检查关卡页/测例/参考解三者一致、
#   编辑器约束行号有效、嵌入内容与仓库一致。
# 所以 `make book-test` 检查的是**这个项目自己的东西**，而不是 Rust 代码块。
book-test: book-build
	cargo test -p neomagic-core selfcheck -- --nocapture
	cargo test -p neomagic-core every_level_page_and_test_config_are_consistent

book-serve:
	cd $(CONTENT_DIR) && mdbook serve --open

# ── Rust (workspace-aware) ──

check:
	cargo check --workspace

test-sim:
	cargo test -p riscv-simulator

# ── Full project ──

test: book-test
	cargo test --workspace

dev: book-build setup
	cd $(APP_DIR) && ./node_modules/.bin/tauri.cmd dev

release: book-build setup
	cd $(APP_DIR) && ./node_modules/.bin/tauri.cmd build

# ── Cleanup ──

clean:
	cargo clean
	rm -rf $(APP_DIR)/src/ayu-highlight*.css
	rm -rf $(APP_DIR)/src/book*.js
	rm -rf $(APP_DIR)/src/clipboard*.js
	rm -rf $(APP_DIR)/src/elasticlunr*.js
	rm -rf $(APP_DIR)/src/highlight*.{css,js}
	rm -rf $(APP_DIR)/src/mark*.js
	rm -rf $(APP_DIR)/src/searcher*.js
	rm -rf $(APP_DIR)/src/searchindex*.js
	rm -rf $(APP_DIR)/src/toc*.js
	rm -rf $(APP_DIR)/src/*.html
	rm -rf $(APP_DIR)/src/*.png
	rm -rf $(APP_DIR)/src/*.svg
	rm -rf $(APP_DIR)/src/css
	rm -rf $(APP_DIR)/src/fonts
	rm -rf $(APP_DIR)/src/theme
