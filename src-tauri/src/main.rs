// Windows 发布版不弹出黑框控制台
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cmddeck_lib::run()
}
