//! 构建脚本：把关卡内容（关卡页 + 测例 + 参考解）编译进二进制。
//!
//! 只有开了 `embedded-content` 特性时才会生成清单文件 `embedded_manifest.rs`
//! （在 `OUT_DIR` 里），内容形如：
//!
//! ```text
//! pub static FILES: &[(&str, &[u8])] = &[("src/c1_1.md", b"..."), ...];
//! ```
//!
//! 为什么不用 `include_dir` 之类的现成 crate：本项目的嵌入需求只有"按相对路径取一段字节"，
//! 一个三十行的构建脚本就够了，还能少一个依赖、少一次网络下载。
//!
//! 同时会把 `compiler-content/**` 全部登记成 `rerun-if-changed`，这样改关卡内容
//! 一定会触发重新编译 —— 否则改完关卡、`cargo run` 却还是旧内容，非常难查。

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    // crates/neomagic-core → 仓库根
    let repo_root = manifest_dir.parent().and_then(|p| p.parent()).unwrap().to_path_buf();
    let content = repo_root.join("compiler-content");

    println!("cargo:rerun-if-changed={}", content.display());

    // 没开特性就不生成清单，但上面的 rerun-if-changed 仍然有用。
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_CONTENT").is_none() {
        return;
    }

    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    // 只嵌"引擎需要读的文件"：关卡页、测例、参考解。
    for sub in ["src", "solutions"] {
        let dir = content.join(sub);
        collect(&dir, &dir, sub, &mut entries);
    }
    entries.sort();

    let mut out = String::new();
    out.push_str("// 由 build.rs 生成，请勿手改。\n");
    out.push_str("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (rel, abs) in &entries {
        println!("cargo:rerun-if-changed={}", abs.display());
        let bytes = std::fs::read(abs)
            .unwrap_or_else(|e| panic!("读不了 {}：{}", abs.display(), e));
        // 用字节字面量而不是字符串字面量：内容里有中文，按字节嵌进来最省事。
        out.push_str(&format!("    ({:?}, &{:?}),\n", rel.replace('\\', "/"), bytes));
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded_manifest.rs");
    std::fs::write(&dest, out).expect("写不了嵌入清单");
}

/// 递归收集目录下的所有文件，键是"`prefix`/相对路径"。
///
/// ⚠ 必须带 `src/` 或 `solutions/` 这类前缀：读取端用的就是
/// `src/c1_1.md`、`solutions/c1_1.s` 这样的相对 `compiler-content/` 的路径。
/// 早期版本这里只写文件名（`c1_1.md`），于是嵌入后端一个文件都查不到 ——
/// 而守卫测试当时因为特性默认关闭从没跑过，谁都没发现。
fn collect(base: &Path, dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, prefix, out);
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            out.push((format!("{}/{}", prefix, rel), path));
        }
    }
}
