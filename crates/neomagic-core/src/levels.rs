//! 关卡文件：读取关卡页（Markdown）、测例（JSON）与参考解，并做自检。
//!
//! 关卡目录约定（见 `documents/关卡编写规范.md`）：
//!
//! ```text
//! compiler-content/src/cN_M.md          ← 关卡页：叙事 + ```riscv 块
//! compiler-content/src/tests/cN_M.json  ← 测例
//! compiler-content/solutions/cN_M.s|c   ← 参考解
//! ```

use crate::config::Language;
use crate::judge::judge;
use std::path::{Path, PathBuf};

/// 一个关卡的编辑器配置（`riscv` 块里的 JSON）。
///
/// 它会被**直接序列化给前端**（Tauri IPC），所以字段名是 camelCase、
/// 正好对上 `compiler-content/frontend/src/core/types.ts` 里的 `EditorConfig`。
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfig {
    pub level: String,
    pub language: Language,
    pub line_numbers: bool,
    /// 前 N 行只读
    pub readonly_prefix: usize,
    /// 后 N 行只读
    pub readonly_suffix: usize,
    /// 隐藏行（1-based）
    pub hidden: Vec<usize>,
    /// 高亮行（1-based）
    pub highlight: Vec<usize>,
    /// 行号 → 提示文本
    pub suggested: std::collections::BTreeMap<usize, String>,
    /// 按钮显隐
    pub toolbar: ToolbarConfig,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbarConfig {
    pub run: bool,
    pub debug: bool,
    pub submit: bool,
    pub load: bool,
    pub save: bool,
}

impl Default for ToolbarConfig {
    fn default() -> Self {
        Self { run: true, debug: true, submit: true, load: true, save: true }
    }
}

/// 一个关卡的静态数据（关卡页解析结果）。
#[derive(Debug, Clone, Default)]
pub struct LevelSource {
    /// 关卡页路径。**嵌入内容后端下这是虚拟路径**（`<嵌入内容>/src/c1_1.md`），
    /// 拿去读磁盘会读到空 —— 要正文请用 [`Self::page_text`]。
    pub path: PathBuf,
    /// 关卡页正文（Markdown 原文），前端用来显示关卡名/侧边栏标题。
    pub page_text: String,
    /// `riscv` 块里的编辑器配置
    pub editor: EditorConfig,
    /// `riscv` 块里 `---` 之后的初始代码
    pub initial_code: String,
}

/// 关卡加载错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelError {
    NotFound(String),
    /// 关卡页的 `riscv` 块 JSON 坏了
    BadEditorConfig { path: PathBuf, message: String },
    /// 测例文件读不到或解析失败
    BadTestConfig { path: PathBuf, message: String },
    /// 解析出的 level 与文件名对不上
    LevelIdMismatch { expected: String, actual: String },
}

impl std::fmt::Display for LevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LevelError::NotFound(id) => write!(f, "找不到关卡 {}", id),
            LevelError::BadEditorConfig { path, message } => {
                write!(f, "{} 的编辑器配置有问题：{}", path.display(), message)
            }
            LevelError::BadTestConfig { path, message } => {
                write!(f, "{} 的测例有问题：{}", path.display(), message)
            }
            LevelError::LevelIdMismatch { expected, actual } => {
                write!(f, "关卡 ID 不一致：文件名是 {}，配置里写的是 {}", expected, actual)
            }
        }
    }
}

/// 一个关卡的完整数据（页面 + 测例 + 参考解）。
#[derive(Debug, Clone)]
pub struct LevelBundle {
    pub id: String,
    pub source: LevelSource,
    pub config: crate::config::LevelConfig,
    /// 参考解源码（没有则 None）
    pub solution: Option<String>,
}

/// 关卡目录（默认是仓库里的 `compiler-content`）。
///
/// 内容可以来自**文件系统**（开发时：改完关卡刷新就能看到），也可以来自
/// **编译期嵌入的字节**（打包后的游戏：玩家机器上没有仓库目录）。
/// 这个差异全部收在 [`Content`] 里，上层的加载逻辑完全一样。
#[derive(Debug, Clone)]
pub struct LevelStore {
    content: Content,
}

/// 内容后端。
#[derive(Debug, Clone)]
enum Content {
    /// 磁盘上的 `compiler-content`
    Dir(PathBuf),
    /// 嵌进二进制的关卡内容（`crates/neomagic-core/build.rs` 生成）
    Embedded,
}

impl Content {
    /// `rel` 是相对 `compiler-content/` 的路径，例如 `src/c1_1.md`。
    fn read(&self, rel: &str) -> Option<String> {
        match self {
            Content::Dir(root) => std::fs::read_to_string(root.join(rel)).ok(),
            Content::Embedded => {
                crate::embedded::read(rel).map(|b| String::from_utf8_lossy(b).to_string())
            }
        }
    }

    fn exists(&self, rel: &str) -> bool {
        match self {
            Content::Dir(root) => root.join(rel).exists(),
            Content::Embedded => crate::embedded::read(rel).is_some(),
        }
    }

    /// 全部关卡（章节号, 关号）。
    ///
    /// 注意这里保留**文件名里的原始数字串**（`c3_01.md` → `("3", "01")`），
    /// 因为测例文件名必须和关卡页同名；早期版本把 `01` 归一成 `1` 再拼
    /// `c3_1.json`，于是带前导零的关卡在嵌入内容里整批消失。
    fn level_files(&self) -> Vec<(String, String)> {
        let mut found: Vec<(u32, u32, String, String)> = Vec::new();
        let mut take = |name: &str| {
            if let Some((cn, mn, cs, ms)) = parse_level_file_name(name) {
                found.push((cn, mn, cs, ms));
            }
        };
        match self {
            Content::Dir(root) => {
                let Ok(entries) = std::fs::read_dir(root.join("src")) else { return Vec::new() };
                for e in entries.flatten() {
                    take(&e.file_name().to_string_lossy());
                }
            }
            Content::Embedded => {
                for rel in crate::embedded::paths() {
                    // 只要 `src/` 下的关卡页，跳过 `src/tests/...` 与其它文件
                    let Some(name) = rel.strip_prefix("src/") else { continue };
                    if name.contains('/') {
                        continue;
                    }
                    take(name);
                }
            }
        }
        // 只有"关卡页 + 测例"都齐了才算一关
        found.retain(|(_, _, c, m)| self.exists(&format!("src/tests/c{}_{}.json", c, m)));
        found.sort();
        found.dedup();
        found.into_iter().map(|(_, _, c, m)| (c, m)).collect()
    }
}

impl LevelStore {
    /// 用 `compiler-content` 目录构造。
    pub fn new(content_dir: impl Into<PathBuf>) -> Self {
        Self { content: Content::Dir(content_dir.into()) }
    }

    /// 用**编译期嵌入**的关卡内容构造（需要 `embedded-content` 特性）。
    pub fn embedded() -> Self {
        assert!(
            cfg!(feature = "embedded-content"),
            "LevelStore::embedded() 需要 neomagic-core 的 embedded-content 特性"
        );
        Self { content: Content::Embedded }
    }

    /// 从仓库根目录推断（向上找 `compiler-content`）。
    pub fn from_repo_root(root: impl AsRef<Path>) -> Self {
        Self::new(root.as_ref().join("compiler-content"))
    }

    /// 定位当前仓库（编译期已知的 `CARGO_MANIFEST_DIR` 往上两级）。
    pub fn default_repo() -> Self {
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // crates/neomagic-core → 仓库根
        let root = here.parent().and_then(|p| p.parent()).unwrap_or(&here).to_path_buf();
        Self::from_repo_root(root)
    }

    /// 开发时优先读仓库里的文件；仓库不在（= 装好的游戏）就读嵌入内容。
    ///
    /// 这条"先盘后嵌"的顺序是刻意的：作者改完关卡立刻能在游戏里看到，
    /// 不用重新编译 Rust。
    pub fn prefer_repo_or_embedded() -> Self {
        let store = Self::default_repo();
        if store.content_root().join("src").is_dir() {
            store
        } else {
            Self::embedded()
        }
    }

    /// 内容根目录；嵌入内容时给一个人可读的占位。
    pub fn content_root(&self) -> PathBuf {
        match &self.content {
            Content::Dir(d) => d.clone(),
            Content::Embedded => PathBuf::from("<嵌入内容>"),
        }
    }

    /// 兼容旧名字。
    pub fn content_dir(&self) -> PathBuf {
        self.content_root()
    }

    /// 全部关卡 ID（按章节与关号排序）。
    pub fn level_ids(&self) -> Vec<String> {
        self.content
            .level_files()
            .into_iter()
            .map(|(c, m)| format!("{}-{}", c, m))
            .collect()
    }

    fn page_rel(&self, id: &str) -> String {
        let (c, m) = split_level_id(id);
        format!("src/c{}_{}.md", c, m)
    }

    fn test_rel(&self, id: &str) -> String {
        let (c, m) = split_level_id(id);
        format!("src/tests/c{}_{}.json", c, m)
    }

    fn solution_rel(&self, id: &str, lang: Language) -> String {
        let (c, m) = split_level_id(id);
        let ext = match lang {
            Language::Asm => "s",
            Language::C => "c",
        };
        format!("solutions/c{}_{}.{}", c, m, ext)
    }

    /// 读取关卡页并解析 `riscv` 块。
    pub fn load_source(&self, id: &str) -> Result<LevelSource, LevelError> {
        let rel = self.page_rel(id);
        let path = self.content_root().join(&rel);
        let Some(text) = self.content.read(&rel) else {
            return Err(LevelError::NotFound(id.to_string()));
        };
        parse_level_page(&text, id, &path)
    }

    /// 读取测例配置。
    pub fn load_config(&self, id: &str) -> Result<crate::config::LevelConfig, LevelError> {
        let rel = self.test_rel(id);
        let path = self.content_root().join(&rel);
        let Some(text) = self.content.read(&rel) else {
            return Err(LevelError::NotFound(id.to_string()));
        };
        let cfg = crate::config::LevelConfig::parse(&text).map_err(|e| LevelError::BadTestConfig {
            path: path.clone(),
            message: e,
        })?;
        if !cfg.level.is_empty() && cfg.level != id && cfg.level != "（未知关卡）" {
            return Err(LevelError::LevelIdMismatch {
                expected: id.to_string(),
                actual: cfg.level,
            });
        }
        Ok(cfg)
    }

    /// 加载一个关卡的完整数据。
    pub fn load(&self, id: &str) -> Result<LevelBundle, LevelError> {
        let source = self.load_source(id)?;
        let mut config = self.load_config(id)?;
        if config.language == Language::Asm && source.editor.language == Language::C {
            config.language = Language::C;
        }
        let solution = self.content.read(&self.solution_rel(id, config.language));
        Ok(LevelBundle { id: id.to_string(), source, config, solution })
    }

    /// 全部关卡的 ID（与 [`Self::level_ids`] 相同，语义更清楚）。
    pub fn all(&self) -> Vec<String> {
        self.level_ids()
    }

    /// **关卡自检**：用参考解跑一遍这个关卡的判定，看是否通过。
    ///
    /// 返回 `Ok(verdict)` 表示自检跑完了（`verdict.passed` 才是"关卡可解"）；
    /// 返回 `Err` 表示连关卡文件/参考解都没齐。
    pub fn self_check(&self, id: &str) -> Result<crate::judge::Verdict, String> {
        let bundle = self.load(id).map_err(|e| e.to_string())?;
        let Some(solution) = &bundle.solution else {
            return Err(format!(
                "关卡 {} 没有参考解（应该放在 compiler-content/solutions/c{}_{}.{}）",
                id,
                split_level_id(id).0,
                split_level_id(id).1,
                if bundle.config.language == Language::C { "c" } else { "s" }
            ));
        };
        Ok(judge(&bundle.config, solution))
    }
}

/// 解析关卡页：找出第一个 ```` ```riscv ```` 块，拆出 JSON 配置与初始代码。
pub fn parse_level_page(text: &str, id: &str, path: &Path) -> Result<LevelSource, LevelError> {
    let normalized = text.replace("\r\n", "\n");
    let mut in_block = false;
    let mut block_lines: Vec<&str> = Vec::new();
    for line in normalized.lines() {
        let trimmed = line.trim_end();
        if !in_block {
            if trimmed.trim_start().starts_with("```riscv") {
                in_block = true;
            }
            continue;
        }
        if trimmed.trim_start().starts_with("```") {
            break;
        }
        block_lines.push(line);
    }
    if block_lines.is_empty() {
        return Err(LevelError::BadEditorConfig {
            path: path.to_path_buf(),
            message: format!("关卡 {} 里没有找到 ```riscv 代码块", id),
        });
    }

    // 用 `---` 单独成行切分配置与代码
    let sep = block_lines.iter().position(|l| l.trim() == "---");
    let (json_text, code_lines): (String, &[&str]) = match sep {
        Some(i) => (block_lines[..i].join("\n"), &block_lines[i + 1..]),
        None => {
            // 兼容：以最后一个 `}` 为界
            let joined = block_lines.join("\n");
            match joined.rfind('}') {
                Some(pos) => (joined[..=pos].to_string(), &[]),
                None => (String::new(), &block_lines[..]),
            }
        }
    };

    let mut editor = EditorConfig { level: id.to_string(), line_numbers: true, ..Default::default() };
    if !json_text.trim().is_empty() {
        let v: serde_json::Value = serde_json::from_str(&json_text).map_err(|e| {
            LevelError::BadEditorConfig { path: path.to_path_buf(), message: e.to_string() }
        })?;
        if let Some(l) = v.get("level").and_then(|x| x.as_str()) {
            editor.level = l.to_string();
        }
        if let Some(l) = v.get("language").and_then(|x| x.as_str()) {
            editor.language = Language::parse(l).ok_or_else(|| LevelError::BadEditorConfig {
                path: path.to_path_buf(),
                message: format!("language 只能是 \"asm\" 或 \"c\"（写的是 {}）", l),
            })?;
        }
        if let Some(b) = v.get("lineNumbers").and_then(|x| x.as_bool()) {
            editor.line_numbers = b;
        }
        if let Some(c) = v.get("constraints") {
            if let Some(n) = c.get("readonlyPrefix").and_then(|x| x.as_u64()) {
                editor.readonly_prefix = n as usize;
            }
            if let Some(n) = c.get("readonlySuffix").and_then(|x| x.as_u64()) {
                editor.readonly_suffix = n as usize;
            }
            editor.hidden = int_list(c.get("hidden"));
            editor.highlight = int_list(c.get("highlight"));
            if let Some(s) = c.get("suggested").and_then(|x| x.as_object()) {
                for (k, val) in s {
                    if let (Ok(line), Some(txt)) = (k.parse::<usize>(), val.as_str()) {
                        editor.suggested.insert(line, txt.to_string());
                    }
                }
            }
        }
        if let Some(t) = v.get("toolbar") {
            let get = |k: &str| t.get(k).and_then(|x| x.as_bool()).unwrap_or(true);
            editor.toolbar = ToolbarConfig {
                run: get("run"),
                debug: get("debug"),
                submit: get("submit"),
                load: get("load"),
                save: get("save"),
            };
        }
    }

    // 初始代码：去掉尾部空行
    let mut code = code_lines.join("\n");
    while code.ends_with('\n') {
        code.pop();
    }

    Ok(LevelSource {
        path: path.to_path_buf(),
        page_text: text.to_string(),
        editor,
        initial_code: code,
    })
}

fn int_list(v: Option<&serde_json::Value>) -> Vec<usize> {
    v.and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as usize).collect())
        .unwrap_or_default()
}

/// 从 `c1_5.md` 这样的文件名解析出 `(1, 5, "1", "5")`。
///
/// 前两个是**用于排序**的数值，后两个是**文件名里的原始数字串**（可能带前导零，
/// 例如 `c3_01.md`）—— 拼测例/参考解路径时必须用原始串。
fn parse_level_file_name(name: &str) -> Option<(u32, u32, String, String)> {
    let stem = name.strip_suffix(".md")?;
    let rest = stem.strip_prefix('c')?;
    let (a, b) = rest.split_once('_')?;
    Some((a.parse().ok()?, b.parse().ok()?, a.to_string(), b.to_string()))
}

/// `"1-5"` → `("1", "5")`（保持原样字符串，避免前导零问题）。
fn split_level_id(id: &str) -> (String, String) {
    match id.split_once('-') {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (id.to_string(), "1".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"
## 1.5 记忆晶阵

这里讲一段故事。

```riscv
{
  "level": "1-5",
  "constraints": { "readonlyPrefix": 1, "suggested": { "3": "lw x5, 0(x1)" } }
}
---
# 把数读进来
li x1, 0x1000
lw x5, 0(x1)
ecall
```

后面还有别的文字。

```riscv
{ "level": "不该被读到" }
```
"#;

    #[test]
    fn parses_editor_config_and_initial_code() {
        let src = parse_level_page(PAGE, "1-5", Path::new("x.md")).unwrap();
        assert_eq!(src.editor.level, "1-5");
        assert_eq!(src.editor.readonly_prefix, 1);
        assert_eq!(src.editor.suggested.get(&3).map(|s| s.as_str()), Some("lw x5, 0(x1)"));
        assert!(src.initial_code.contains("li x1, 0x1000"));
        assert!(!src.initial_code.contains("ecall\n\n"), "尾部空行要去掉");
        assert!(!src.initial_code.contains("不该被读到"), "只取第一个 riscv 块");
    }

    #[test]
    fn missing_block_is_reported() {
        let e = parse_level_page("没有代码块的页面", "1-1", Path::new("x.md")).unwrap_err();
        assert!(matches!(e, LevelError::BadEditorConfig { .. }));
    }

    #[test]
    fn bad_json_is_reported() {
        let text = "```riscv\n{ 这不是 JSON\n---\nnop\n```\n";
        let e = parse_level_page(text, "1-1", Path::new("x.md")).unwrap_err();
        assert!(matches!(e, LevelError::BadEditorConfig { .. }));
    }

    #[test]
    fn level_file_names_are_parsed() {
        // 返回四元组：(章节号, 关号, 原始章节串, 原始关号串)
        assert_eq!(
            parse_level_file_name("c1_5.md"),
            Some((1, 5, "1".to_string(), "5".to_string()))
        );
        assert_eq!(
            parse_level_file_name("c12_3.md"),
            Some((12, 3, "12".to_string(), "3".to_string()))
        );
        // 带前导零的文件名要保留原始串，否则拼不出测例路径
        assert_eq!(
            parse_level_file_name("c3_01.md"),
            Some((3, 1, "3".to_string(), "01".to_string()))
        );
        assert_eq!(parse_level_file_name("c1.md"), None);
        assert_eq!(parse_level_file_name("序章_剧情版.md"), None);
    }

    #[test]
    fn level_ids_are_parsed() {
        assert_eq!(split_level_id("1-5"), ("1".into(), "5".into()));
        assert_eq!(split_level_id("12-3"), ("12".into(), "3".into()));
    }
}
