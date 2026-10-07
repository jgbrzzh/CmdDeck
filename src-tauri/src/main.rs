// Windows 发布版不弹出黑框控制台
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if cmddeck_lib::elevated::worker_entry() {
        return;
    }
    cmddeck_lib::run()
}
