// 构建脚本：告诉 Tauri 在编译前重新生成 Windows 资源与插件权限表。
// 如果不存在 tauri_build::build()，Windows 下图标/清单不会被正确嵌入。

fn main() {
    // 图标和清单变化时重建资源，避免开发模式只重启却继续使用旧 EXE 图标。
    for path in [
        "app.manifest",
        "icons/icon.ico",
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    // 显式使用当前用户权限，避免 Windows 的安装程序检测触发意外提权。
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("生成 Windows 资源失败");
}
