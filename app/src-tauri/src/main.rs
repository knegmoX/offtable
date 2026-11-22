// 桌面入口（仅开发用；发行走移动端与 PWA）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    offtable_app_lib::run()
}
