#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Minagi 文件分类助手 v2.0 — 程序入口
/// 这个文件很小，真正的工作都在 lib.rs 里哦

fn main() {
    minagi_file_classifier::run();
}
