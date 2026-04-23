use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // 注入构建时间戳（epoch 秒），供 /sdk/manifest.json 的 builtAt 字段使用。
    // 因为下面还声明了 rerun-if-changed，只有 SDK 产物或本文件变化时才会刷新，
    // 不会每次 cargo build 都改动，避免污染增量编译。
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=BUILD_TIMESTAMP={ts}");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let dist_dir = Path::new(&manifest_dir).join("../../sdk/dist");
    let pkg_dir = Path::new(&manifest_dir).join("../../sdk/pkg");

    if !dist_dir.exists() || !pkg_dir.exists() {
        println!("cargo:warning=静态资源目录不存在。请先运行构建脚本：");
        println!("cargo:warning=  bash scripts/build-all.sh");
        println!("cargo:warning=  或手动: wasm-pack build + cd sdk && pnpm build");
        // 不 panic —— 允许在没有前端产物时编译（集成测试场景下 dist 可能为空）
    }

    // 当 SDK 产物变化时重新编译（仅在资源目录存在时）
    if dist_dir.exists() {
        println!("cargo:rerun-if-changed=../../sdk/dist");
    }
    if pkg_dir.exists() {
        println!("cargo:rerun-if-changed=../../sdk/pkg");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
