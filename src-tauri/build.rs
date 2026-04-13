// TabPilot build script — 入口
//
// 各构建模块放在 build_scripts/ 目录下:
//   build_scripts/env.rs  — 编译时嵌入 env 变量
//   build_scripts/cdp.rs  — CDP protocol codegen

#[path = "build_scripts/cdp.rs"]
mod cdp;
#[path = "build_scripts/env.rs"]
mod env;

fn main() {
    tauri_build::build();
    env::embed();
    cdp::generate();
}
