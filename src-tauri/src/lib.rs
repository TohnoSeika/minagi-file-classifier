/// Minagi 文件分类助手 v2.0 — Tauri 版 主进程
/// 这个文件是桃华帮 Minagi 从 Electron 迁移到 Tauri 的哦 (*´▽`*)
///
/// 和 Electron 版相比：
///   - 后端从 Node.js 换成了 Rust，文件操作更快更安全
///   - 不再打包 Chromium，体积从 366MB 降到 ~10MB
///   - 内存占用从 ~200MB 降到 ~50MB

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
    Emitter, Manager,
};

// ─── Windows 原生 FFI（窗口透明度） ─────────────
#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;
    pub type HWND = *mut c_void;
    pub type COLORREF = u32;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const WS_EX_LAYERED: u32 = 0x00080000;
    pub const LWA_ALPHA: u32 = 0x00000002;
    extern "system" {
        pub fn GetWindowLongPtrW(hwnd: HWND, nIndex: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: HWND, nIndex: i32, dwNewLong: isize) -> isize;
        pub fn SetLayeredWindowAttributes(hwnd: HWND, crKey: COLORREF, bAlpha: u8, dwFlags: u32) -> i32;
    }
}

// ─── 快照常量 ─────────────────────────────────────
// 桃华定制：配置快照机制 —— 每次重要设置变更自动存档
const SNAPSHOTS_DIR: &str = "snapshots";
const MAX_SNAPSHOTS: usize = 7;

// ─── 数据结构 ─────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Zone {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    bg: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Config {
    #[serde(default = "default_opacity")]
    opacity: f64,
    #[serde(default = "default_bg_opacity")]
    bg_opacity: f64,
    #[serde(default)]
    global_bg: String,
    #[serde(default = "default_theme_color")]
    theme_color: String,
    #[serde(default = "default_true")]
    chime_enabled: bool,
    #[serde(default)]
    minimize_to_tray: bool,
    #[serde(default = "default_true")]
    close_to_tray: bool,
    #[serde(default = "default_zones")]
    zones: Vec<Zone>,
}

fn default_opacity() -> f64 {
    0.95
}

fn default_bg_opacity() -> f64 {
    0.40
}

fn default_theme_color() -> String {
    "#e87890".to_string()
}

fn default_true() -> bool {
    true
}

fn default_zones() -> Vec<Zone> {
    (1..=6)
        .map(|i| Zone {
            id: i,
            name: format!("区域 {}", i),
            path: String::new(),
            bg: String::new(),
        })
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            opacity: 0.95,
            bg_opacity: 0.40,
            global_bg: String::new(),
            theme_color: "#e87890".to_string(),
            chime_enabled: true,
            minimize_to_tray: false,
            close_to_tray: true,
            zones: default_zones(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct HistoryEntry {
    timestamp: u64,
    source_paths: Vec<String>,
    target_dir: String,
    mode: String,
    file_names: Vec<String>,
    item_count: usize,
}

#[derive(Debug, Serialize, Clone)]
struct ProgressPayload {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    percent: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

// ─── 应用状态 ─────────────────────────────────────

/// 取消令牌 —— 文件操作中的取消标记
struct CancelState(Arc<AtomicBool>);

/// 任务锁 —— 避免多个复制/剪切任务互相踩进度
struct TaskState(Arc<AtomicBool>);

/// 配置缓存 —— 避免频繁读文件
struct ConfigState(Mutex<Config>);

/// 操作历史 —— 最多 50 条
struct HistoryState(Mutex<Vec<HistoryEntry>>);

// ─── 工具函数 ─────────────────────────────────────

/// 生成不重复的目标路径，避免覆盖已有文件
fn get_unique_path(target_path: &Path) -> PathBuf {
    if !target_path.exists() {
        return target_path.to_path_buf();
    }

    let parent = target_path.parent().unwrap_or(Path::new("."));
    let ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let stem = target_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut count = 1u32;
    loop {
        let name = if ext.is_empty() {
            format!("{} ({})", stem, count)
        } else {
            format!("{} ({}).{}", stem, count, ext)
        };
        let candidate = parent.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        count += 1;
    }
}

/// 递归统计文件/文件夹内的总字节数
fn count_bytes(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => {
            fs::read_dir(path)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .map(|e| count_bytes(&e.path()))
                        .sum()
                })
                .unwrap_or(0)
        }
        Ok(meta) => meta.len(),
        Err(_) => 0,
    }
}

fn count_bytes_bulk(paths: &[String]) -> u64 {
    paths.iter().map(|p| count_bytes(Path::new(p))).sum()
}

// ═══════════════════════════════════════════════════
// 桃华定制：配置快照机制
// 每次用户改区域名/路径/背景图时自动存档，最多保留 10 份
// ═══════════════════════════════════════════════════

/// 创建一份配置快照（config.json + 当前所有背景图片）
/// 目录名为 Unix 时间戳，前端用 JS Date 格式化为本地时间
fn create_snapshot(data_dir: &Path) {
    let config_file = data_dir.join("config.json");
    let images_dir = data_dir.join("images");
    let snapshots_dir = data_dir.join(SNAPSHOTS_DIR);

    if !config_file.exists() {
        return;
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let snap_name = ts.to_string();
    let snap_dir = snapshots_dir.join(&snap_name);
    if snap_dir.exists() {
        return;
    }

    let _ = fs::create_dir_all(&snap_dir);
    let _ = fs::copy(&config_file, snap_dir.join("config.json"));

    if images_dir.exists() {
        let snap_images = snap_dir.join("images");
        let _ = fs::create_dir_all(&snap_images);
        copy_dir_content(&images_dir, &snap_images);
    }

    cleanup_snapshots(&snapshots_dir);
}

/// 递归复制目录内容
fn copy_dir_content(src: &Path, dst: &Path) {
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dest = dst.join(entry.file_name());
            if path.is_dir() {
                let _ = fs::create_dir_all(&dest);
                copy_dir_content(&path, &dest);
            } else {
                let _ = fs::copy(&path, &dest);
            }
        }
    }
}

// ── 桃华注：AppData 共用 minagi-file-classifier，无需迁移 ──

/// 保留最近 MAX_SNAPSHOTS 份，删除更旧的
fn cleanup_snapshots(snapshots_dir: &Path) {
    if !snapshots_dir.exists() {
        return;
    }

    let mut dirs: Vec<PathBuf> = fs::read_dir(snapshots_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.file_name().to_string_lossy().parse::<u64>().is_ok()
        })
        .map(|e| e.path())
        .collect();

    // 按时间戳数字大小降序
    dirs.sort_by(|a, b| {
        let ta = a.file_name().and_then(|n| n.to_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let tb = b.file_name().and_then(|n| n.to_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        tb.cmp(&ta)
    });

    for old in dirs.iter().skip(MAX_SNAPSHOTS) {
        let _ = fs::remove_dir_all(old);
    }
}

/// 启动时尝试从最新快照自动恢复 config.json
/// 返回 true 表示已恢复
fn try_auto_recover(data_dir: &Path) -> bool {
    let config_file = data_dir.join("config.json");
    let snapshots_dir = data_dir.join(SNAPSHOTS_DIR);

    // config.json 正常就不需要恢复
    if config_file.exists() {
        if let Ok(content) = fs::read_to_string(&config_file) {
            if serde_json::from_str::<Config>(&content).is_ok() {
                return false;
            }
        }
    }

    // 从快照恢复
    if !snapshots_dir.exists() {
        return false;
    }

    let mut dirs: Vec<PathBuf> = fs::read_dir(&snapshots_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.file_name().to_string_lossy().parse::<u64>().is_ok()
        })
        .map(|e| e.path())
        .collect();

    dirs.sort_by(|a, b| {
        let ta = a.file_name().and_then(|n| n.to_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        let tb = b.file_name().and_then(|n| n.to_str()).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        tb.cmp(&ta)
    });

    for snap in &dirs {
        let snap_config = snap.join("config.json");
        if snap_config.exists() {
            if let Ok(content) = fs::read_to_string(&snap_config) {
                if serde_json::from_str::<Config>(&content).is_ok() {
                    // 恢复 config.json
                    let _ = fs::write(&config_file, &content);
                    // 恢复图片
                    let snap_images = snap.join("images");
                    if snap_images.exists() {
                        let images_dir = data_dir.join("images");
                        let _ = fs::remove_dir_all(&images_dir);
                        let _ = fs::create_dir_all(&images_dir);
                        copy_dir_content(&snap_images, &images_dir);
                    }
                    return true;
                }
            }
        }
    }

    false
}

// ─── 文件拷贝（带进度 + 取消支持） ─────────────────

/// 流式拷贝单个文件，每 256KB 回报一次写入字节数
fn copy_file_with_progress(
    source: &Path,
    dest: &Path,
    on_bytes: &mut dyn FnMut(u64),
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut src = fs::File::open(source).map_err(|e| format!("无法读取文件: {}", e))?;
    let file_size = src.metadata().map_err(|e| e.to_string())?.len();

    let mut dst = fs::File::create(dest).map_err(|e| format!("无法创建文件: {}", e))?;
    let mut buf = [0u8; 256 * 1024]; // 256KB 缓冲区
    let mut total_written: u64 = 0;
    let mut last_report: u64 = 0;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = fs::remove_file(dest); // 清理未完成的文件
            return Err("cancelled".to_string());
        }

        let bytes_read = src.read(&mut buf).map_err(|e| format!("读取失败: {}", e))?;
        if bytes_read == 0 {
            break;
        }

        dst.write_all(&buf[..bytes_read])
            .map_err(|e| format!("写入失败: {}", e))?;
        total_written += bytes_read as u64;

        let delta = total_written - last_report;
        if delta >= 256 * 1024 || total_written == file_size {
            on_bytes(delta);
            last_report = total_written;
        }
    }

    // 最后上报剩余字节
    if total_written > last_report {
        on_bytes(total_written - last_report);
    }

    Ok(())
}

/// 递归拷贝文件夹，边走边回调已写入字节数
fn copy_folder_recursive(
    source: &Path,
    dest: &Path,
    on_bytes: &mut dyn FnMut(u64),
    cancelled: &AtomicBool,
) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }

    let meta = fs::metadata(source).map_err(|e| format!("无法访问: {}", e))?;

    if meta.is_dir() {
        fs::create_dir_all(dest).map_err(|e| format!("无法创建目录: {}", e))?;

        let entries = fs::read_dir(source).map_err(|e| format!("无法读取目录: {}", e))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_folder_recursive(
                &entry.path(),
                &dest.join(entry.file_name()),
                on_bytes,
                cancelled,
            )?;
        }
    } else {
        copy_file_with_progress(source, dest, on_bytes, cancelled)?;
    }

    Ok(())
}

// ─── 文件移动/复制核心逻辑 ─────────────────────

/// 在后台执行文件移动/复制，通过事件系统推送进度
async fn do_move_files(
    app_handle: tauri::AppHandle,
    file_paths: Vec<String>,
    target_dir: String,
    mode: String,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
) {
    let target = Path::new(&target_dir);
    if !target.exists() || !target.is_dir() {
        let _ = app_handle.emit(
            "task-progress",
            ProgressPayload {
                status: "error".into(),
                total: None,
                percent: None,
                current: None,
                error: Some("目标文件夹不存在呢……".into()),
                count: None,
            },
        );
        running.store(false, Ordering::SeqCst);
        return;
    }

    let total_bytes = count_bytes_bulk(&file_paths);

    let _ = app_handle.emit(
        "task-progress",
        ProgressPayload {
            status: "start".into(),
            total: Some(total_bytes),
            percent: None,
            current: None,
            error: None,
            count: None,
        },
    );

    let mut bytes_done: u64 = 0;
    let app_for_events = app_handle.clone();
    let mut last_send = std::time::Instant::now();

    let mut on_bytes = |delta: u64| {
        bytes_done += delta;
        let now = std::time::Instant::now();
        // 节流：最多每 60ms 推送一次进度
        if now.duration_since(last_send).as_millis() >= 60 || bytes_done >= total_bytes {
            let percent = if total_bytes > 0 {
                ((bytes_done as f64 / total_bytes as f64) * 100.0) as u32
            } else {
                0
            };
            let _ = app_for_events.emit(
                "task-progress",
                ProgressPayload {
                    status: "progress".into(),
                    total: Some(total_bytes),
                    percent: Some(percent),
                    current: Some(bytes_done),
                    error: None,
                    count: None,
                },
            );
            last_send = now;
        }
    };

    for source_path_str in &file_paths {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let source = Path::new(source_path_str);
        let base_name = source.file_name().unwrap_or_default();
        let raw_dest = target.join(base_name);
        let dest_path = get_unique_path(&raw_dest);

        let mut moved = false;

        // 剪切模式优先尝试 rename（同盘瞬间完成）
        if mode == "cut" && !cancelled.load(Ordering::Relaxed) {
            if fs::rename(source, &dest_path).is_ok() {
                let size = count_bytes(&dest_path);
                on_bytes(size);
                moved = true;
            }
        }

        // rename 失败或复制模式：走流式拷贝
        if !moved && !cancelled.load(Ordering::Relaxed) {
            if let Err(e) =
                copy_folder_recursive(source, &dest_path, &mut on_bytes, &cancelled)
            {
                if e == "cancelled" {
                    break;
                }
                let _ = app_handle.emit(
                    "task-progress",
                    ProgressPayload {
                        status: "error".into(),
                        total: None,
                        percent: None,
                        current: None,
                        error: Some(e),
                        count: None,
                    },
                );
                running.store(false, Ordering::SeqCst);
                return;
            }

            // 剪切模式下拷贝完成后删除源文件
            if mode == "cut" && !cancelled.load(Ordering::Relaxed) {
                let _ = fs::remove_dir_all(source);
                let _ = fs::remove_file(source); // 单个文件也尝试
            }
        }
    }

    if cancelled.load(Ordering::Relaxed) {
        let _ = app_handle.emit(
            "task-progress",
            ProgressPayload {
                status: "cancelled".into(),
                total: None,
                percent: None,
                current: None,
                error: None,
                count: None,
            },
        );
        running.store(false, Ordering::SeqCst);
    } else {
        // ── 记录操作历史 ──
        let file_names: Vec<String> = file_paths
            .iter()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        let entry = HistoryEntry {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            source_paths: file_paths.clone(),
            target_dir: target_dir.clone(),
            mode: mode.clone(),
            file_names,
            item_count: file_paths.len(),
        };
        save_history(&app_handle, entry);

        let _ = app_handle.emit(
            "task-progress",
            ProgressPayload {
                status: "end".into(),
                total: None,
                percent: None,
                current: None,
                error: None,
                count: Some(file_paths.len()),
            },
        );
        running.store(false, Ordering::SeqCst);
    }
}

// ═══════════════════════════════════════════════════
// Tauri 命令 —— 前端通过 invoke() 调用的接口
// ═══════════════════════════════════════════════════

#[tauri::command]
async fn load_config(app_handle: tauri::AppHandle) -> Result<Config, String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_file = config_dir.join("MinagiData").join("config.json");

    let config = if config_file.exists() {
        let content = fs::read_to_string(&config_file).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    } else {
        Config::default()
    };

    // 更新缓存
    if let Some(state) = app_handle.try_state::<ConfigState>() {
        if let Ok(mut cached) = state.0.lock() {
            *cached = config.clone();
        }
    }

    Ok(config)
}

#[tauri::command]
async fn save_config(app_handle: tauri::AppHandle, config: Config, do_snapshot: Option<bool>) -> Result<(), String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let config_file = data_dir.join("config.json");
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&config_file, json).map_err(|e| e.to_string())?;

    // 更新缓存
    if let Some(state) = app_handle.try_state::<ConfigState>() {
        if let Ok(mut cached) = state.0.lock() {
            *cached = config;
        }
    }

    // 区域名/路径/背景图变更时创建快照
    if do_snapshot.unwrap_or(false) {
        create_snapshot(&data_dir);
    }

    Ok(())
}

#[tauri::command]
async fn move_files(
    app_handle: tauri::AppHandle,
    file_paths: Vec<String>,
    target_dir: String,
    mode: String,
) -> Result<(), String> {
    if file_paths.is_empty() {
        return Err("没有可处理的文件".into());
    }
    if mode != "copy" && mode != "cut" {
        return Err("未知操作模式".into());
    }

    let running = {
        let state = app_handle.state::<TaskState>();
        if state
            .0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("正在处理上一批文件，等一下……".into());
        }
        state.0.clone()
    };

    // 重置取消令牌
    let cancel_flag = {
        let state = app_handle.state::<CancelState>();
        state.0.store(false, Ordering::SeqCst);
        state.0.clone()
    };

    // 后台执行，不阻塞前端
    let handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        do_move_files(handle, file_paths, target_dir, mode, cancel_flag, running).await;
    });

    Ok(())
}

#[tauri::command]
async fn cancel_move(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<CancelState>();
    state.0.store(true, Ordering::SeqCst);
    Ok(())
}

/// 保存历史记录到磁盘（最多 50 条）
fn save_history(app_handle: &tauri::AppHandle, entry: HistoryEntry) {
    let Ok(config_dir) = app_handle.path().app_data_dir() else {
        return;
    };
    let data_dir = config_dir.join("MinagiData");
    let history_file = data_dir.join("history.json");
    let _ = fs::create_dir_all(&data_dir);

    // 读现有记录 → 加新记录 → 截断到 50 条 → 写回
    let mut history: Vec<HistoryEntry> = if history_file.exists() {
        fs::read_to_string(&history_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    history.push(entry);
    if history.len() > 50 {
        history = history.split_off(history.len() - 50); // 保留最后 50 条
    }

    // 更新内存缓存
    if let Some(state) = app_handle.try_state::<HistoryState>() {
        if let Ok(mut cached) = state.0.lock() {
            *cached = history.clone();
        }
    }

    let _ = fs::write(&history_file, serde_json::to_string_pretty(&history).unwrap_or_default());
}

#[tauri::command]
async fn load_history(app_handle: tauri::AppHandle) -> Result<Vec<HistoryEntry>, String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let history_file = config_dir.join("MinagiData").join("history.json");

    if history_file.exists() {
        let content = fs::read_to_string(&history_file).map_err(|e| e.to_string())?;
        let history: Vec<HistoryEntry> =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        // 更新缓存
        if let Some(state) = app_handle.try_state::<HistoryState>() {
            if let Ok(mut cached) = state.0.lock() {
                *cached = history.clone();
            }
        }
        Ok(history)
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn clear_history(app_handle: tauri::AppHandle) -> Result<(), String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    fs::write(data_dir.join("history.json"), "[]").map_err(|e| e.to_string())?;

    if let Some(state) = app_handle.try_state::<HistoryState>() {
        if let Ok(mut cached) = state.0.lock() {
            cached.clear();
        }
    }

    Ok(())
}

/// 桃华定制：快照列表的返回结构
/// timestamp 为 Unix 秒数，前端用 new Date(timestamp*1000) 格式化为本地时间
#[derive(Debug, Serialize, Clone)]
struct SnapshotInfo {
    name: String,
    timestamp: u64,
    config_size: u64,
    image_count: usize,
}

#[tauri::command]
async fn list_snapshots(app_handle: tauri::AppHandle) -> Result<Vec<SnapshotInfo>, String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let snapshots_dir = config_dir.join("MinagiData").join(SNAPSHOTS_DIR);

    if !snapshots_dir.exists() {
        return Ok(Vec::new());
    }

    let mut list: Vec<SnapshotInfo> = Vec::new();

    if let Ok(entries) = fs::read_dir(&snapshots_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // 目录名必须是 Unix 时间戳数字，旧格式跳过
            let timestamp: u64 = match name.parse() {
                Ok(ts) => ts,
                Err(_) => continue,
            };

            let config_path = path.join("config.json");
            let config_size = fs::metadata(&config_path)
                .map(|m| m.len())
                .unwrap_or(0);

            let images_dir = path.join("images");
            let image_count = if images_dir.exists() {
                fs::read_dir(&images_dir)
                    .map(|rd| rd.flatten().count())
                    .unwrap_or(0)
            } else {
                0
            };

            list.push(SnapshotInfo {
                name,
                timestamp,
                config_size,
                image_count,
            });
        }
    }

    // 按时间戳降序（最新的在前）
    list.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(list)
}

#[tauri::command]
async fn restore_snapshot(
    app_handle: tauri::AppHandle,
    snapshot_name: String,
) -> Result<(), String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    let snap_dir = data_dir.join(SNAPSHOTS_DIR).join(&snapshot_name);

    if !snap_dir.exists() {
        return Err("快照不存在呢……".into());
    }

    let snap_config = snap_dir.join("config.json");
    if !snap_config.exists() {
        return Err("快照中没有配置文件".into());
    }

    // 验证 JSON 有效
    let content = fs::read_to_string(&snap_config).map_err(|e| e.to_string())?;
    serde_json::from_str::<Config>(&content).map_err(|e| format!("快照配置损坏: {}", e))?;

    // 复制 config.json 到工作目录
    fs::write(data_dir.join("config.json"), &content).map_err(|e| e.to_string())?;

    // 复制 images 到工作目录
    let snap_images = snap_dir.join("images");
    if snap_images.exists() {
        let images_dir = data_dir.join("images");
        let _ = fs::remove_dir_all(&images_dir);
        let _ = fs::create_dir_all(&images_dir);
        copy_dir_content(&snap_images, &images_dir);
    }

    // 更新内存缓存
    if let Some(state) = app_handle.try_state::<ConfigState>() {
        if let Ok(mut cached) = state.0.lock() {
            if let Ok(cfg) = serde_json::from_str(&content) {
                *cached = cfg;
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn open_folder(path: String) -> Result<(), String> {
    if Path::new(&path).exists() {
        open::that(&path).map_err(|e| format!("无法打开文件夹: {}", e))
    } else {
        Err("目标路径不存在哦，检查一下？".into())
    }
}

#[tauri::command]
async fn import_image(
    app_handle: tauri::AppHandle,
    file_name: String,
    file_data_b64: String,
) -> Result<String, String> {
    use base64::Engine;
    const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

    if file_data_b64.is_empty() {
        return Ok(String::new());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&file_data_b64)
        .map_err(|e| format!("解码失败: {}", e))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("图片太大了，换一张 10MB 以内的吧".into());
    }

    // 从文件名中提取扩展名
    let ext = Path::new(&file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");

    let ext_lc = ext.to_lowercase();
    let ext = match ext_lc.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" => ext_lc.as_str(),
        _ => return Err("只支持常见图片格式".into()),
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let suffix = (timestamp % 99999) as u32;
    let new_name = format!("img_{}_{}.{}", timestamp, suffix, ext);

    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let images_dir = config_dir.join("MinagiData").join("images");
    fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    let dest = images_dir.join(&new_name);
    fs::write(&dest, &bytes).map_err(|e| format!("无法保存图片: {}", e))?;

    Ok(dest.to_string_lossy().replace('\\', "/"))
}

#[tauri::command]
async fn win_minimize(
    app_handle: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    let minimize_to_tray = app_handle
        .try_state::<ConfigState>()
        .and_then(|s| s.0.lock().ok().map(|c| c.minimize_to_tray))
        .unwrap_or(false);

    if minimize_to_tray {
        window.hide().map_err(|e| e.to_string())?;
    } else {
        window.minimize().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn win_maximize(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn win_close(
    app_handle: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    let close_to_tray = app_handle
        .try_state::<ConfigState>()
        .and_then(|s| s.0.lock().ok().map(|c| c.close_to_tray))
        .unwrap_or(true);

    if close_to_tray {
        window.hide().map_err(|e| e.to_string())?;
    } else {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn win_move(window: tauri::WebviewWindow, dx: f64, dy: f64) -> Result<(), String> {
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    window
        .set_position(tauri::LogicalPosition::new(
            pos.x as f64 + dx,
            pos.y as f64 + dy,
        ))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_opacity(window: tauri::WebviewWindow, opacity: f64) -> Result<(), String> {
    let clamped = opacity.max(0.1).min(1.0);
    #[cfg(target_os = "windows")]
    {
        use win32::*;
        let raw = window.hwnd().map_err(|e| e.to_string())?;
        // Tauri 返回 windows crate 的 HWND（repr(transparent) over *mut c_void）
        let hwnd: *mut std::ffi::c_void = raw.0;
        unsafe {
            let mut ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            ex_style |= WS_EX_LAYERED as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
            let alpha = (clamped * 255.0) as u8;
            SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
        }
    }
    // 同时设置 CSS 变量，让玻璃面板等跟随变化
    let js = format!(
        "document.documentElement.style.setProperty('--glass-opacity', '{}');",
        clamped
    );
    window.eval(&js).map_err(|e| e.to_string())?;
    Ok(())
}

/// 读取图片文件，返回 base64 data URL（用于 CSS background-image）
#[tauri::command]
async fn read_image_data_url(path: String) -> Result<String, String> {
    use base64::Engine;
    let bytes = std::fs::read(&path).map_err(|e| format!("无法读取图片: {}", e))?;
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let mime = match ext.to_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/jpeg",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

#[tauri::command]
async fn open_external(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("无法打开链接: {}", e))
}

// ═══════════════════════════════════════════════════
// 应用入口
// ═══════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let default_config = Config::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 第二个实例启动时，聚焦已有窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
        }))
        .manage(CancelState(Arc::new(AtomicBool::new(false))))
        .manage(TaskState(Arc::new(AtomicBool::new(false))))
        .manage(ConfigState(Mutex::new(default_config)))
        .manage(HistoryState(Mutex::new(Vec::new())))
        // ── 注册 bgimg:// 协议，加载本地背景图片 ──
        .register_uri_scheme_protocol("bgimg", |ctx, request| {
            let uri = request.uri();
            let path = uri.path();
            let filename = path.trim_start_matches('/');
            let images_dir = ctx
                .app_handle()
                .path()
                .app_data_dir()
                .unwrap()
                .join("MinagiData")
                .join("images");
            let file_path = images_dir.join(filename);

            match std::fs::read(&file_path) {
                Ok(bytes) => {
                    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                    let mime = match ext.to_lowercase().as_str() {
                        "png" => "image/png",
                        "gif" => "image/gif",
                        "webp" => "image/webp",
                        "bmp" => "image/bmp",
                        _ => "image/jpeg",
                    };
                    tauri::http::Response::builder()
                        .status(200)
                        .header("content-type", mime)
                        .body(bytes)
                        .unwrap()
                }
                Err(_) => tauri::http::Response::builder()
                    .status(404)
                    .body(Vec::new())
                    .unwrap(),
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            move_files,
            cancel_move,
            open_folder,
            import_image,
            read_image_data_url,
            load_history,
            clear_history,
            win_minimize,
            win_maximize,
            win_close,
            win_move,
            set_opacity,
            open_external,
            list_snapshots,
            restore_snapshot,
        ])
        .setup(|app| {
            // ── 初始化数据目录 ─────────────────
            let config_dir = app.path().app_data_dir()?;
            let data_dir = config_dir.join("MinagiData");
            let images_dir = data_dir.join("images");
            fs::create_dir_all(&images_dir)?;

            // ── 桃华注：identifier 与 Electron 1.6 一致（minagi-file-classifier），直接共用 ──

            // 桃华定制：首次启动时重置透明度 + 创建快照
            // Electron 1.6 默认透明度 0.65 → 改为 Tauri 默认 0.95 / 0.40
            // 用 .first_snapshot_done 标记防止重复
            // 桃华修复：用 serde_json::Value 而不是 Config 反序列化，避免丢弃旧版配置中的未知字段
            let first_snap_marker = data_dir.join(".first_snapshot_done");
            if !first_snap_marker.exists() && data_dir.join("config.json").exists() {
                let cfg_path = data_dir.join("config.json");
                if let Ok(content) = fs::read_to_string(&cfg_path) {
                    // 用 Value 保留所有旧字段（兼容旧版可能的不同字段名）
                    if let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(obj) = root.as_object_mut() {
                            // 只改透明度字段，其他原封不动
                            obj.insert("opacity".to_string(), serde_json::json!(0.95));
                            obj.insert("bgOpacity".to_string(), serde_json::json!(0.40));
                        }
                        if let Ok(json) = serde_json::to_string_pretty(&root) {
                            let _ = fs::write(&cfg_path, json);
                        }
                    }
                }
                create_snapshot(&data_dir);
                let _ = fs::write(&first_snap_marker, "1");
            }

            // 桃华定制：验证并修复背景图路径 + 迁移旧版配置
            // 如果 config.json 中的背景图片文件不存在，尝试从本地 images 目录中查找并修复为正确的当前绝对路径
            // 同时也处理旧版可能的 data URL 存储方式
            let config_file = data_dir.join("config.json");
            if config_file.exists() {
                let content = fs::read_to_string(&config_file).unwrap_or_default();
                let mut config_modified = false;

                // 第一步：检查旧版配置中是否有替代字段名，迁移到新版字段
                if let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(obj) = root.as_object_mut() {
                        // 如果 globalBg 为空或不存在，检查旧版可能的字段名
                        let has_global_bg = obj.get("globalBg")
                            .and_then(|v| v.as_str())
                            .map(|s| !s.is_empty())
                            .unwrap_or(false);

                        if !has_global_bg {
                            // 尝试旧版可能的字段名：bg（顶层）、backgroundImage、bgImage
                            for old_key in &["bg", "backgroundImage", "bgImage"] {
                                if let Some(val) = obj.remove(*old_key) {
                                    if let Some(s) = val.as_str() {
                                        if !s.is_empty() {
                                            obj.insert("globalBg".to_string(), val);
                                            config_modified = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // 检查区域里是否有旧版字段名（如 backgroundImage 代替 bg）
                        if let Some(zones) = obj.get_mut("zones") {
                            if let Some(zones_arr) = zones.as_array_mut() {
                                for zone in zones_arr {
                                    if let Some(z_obj) = zone.as_object_mut() {
                                        let has_bg = z_obj.get("bg")
                                            .and_then(|v| v.as_str())
                                            .map(|s| !s.is_empty())
                                            .unwrap_or(false);
                                        if !has_bg {
                                            if let Some(val) = z_obj.remove("backgroundImage") {
                                                if let Some(s) = val.as_str() {
                                                    if !s.is_empty() {
                                                        z_obj.insert("bg".to_string(), val);
                                                        config_modified = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if config_modified {
                        if let Ok(json) = serde_json::to_string_pretty(&root) {
                            let _ = fs::write(&config_file, json);
                        }
                    }
                }

                // 第二步：修复图片路径（重新读取已修复的配置）
                let content = fs::read_to_string(&config_file).unwrap_or_default();
                if let Ok(mut cfg) = serde_json::from_str::<Config>(&content) {
                    let mut changed = false;

                    // 修复全局背景图路径
                    let bg = cfg.global_bg.clone();
                    if !bg.is_empty() && !Path::new(&bg).exists() {
                        // 尝试从文件名找
                        if let Some(filename) = Path::new(&bg).file_name() {
                            let new_path = images_dir.join(filename);
                            if new_path.exists() {
                                cfg.global_bg = new_path.to_string_lossy().replace('\\', "/");
                                changed = true;
                            }
                        }
                    }

                    // 修复每个区域的背景图路径
                    for zone in &mut cfg.zones {
                        let zone_bg = zone.bg.clone();
                        if !zone_bg.is_empty() && !Path::new(&zone_bg).exists() {
                            if let Some(filename) = Path::new(&zone_bg).file_name() {
                                let new_path = images_dir.join(filename);
                                if new_path.exists() {
                                    zone.bg = new_path.to_string_lossy().replace('\\', "/");
                                    changed = true;
                                }
                            }
                        }
                    }

                    if changed {
                        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                            let _ = fs::write(&config_file, json);
                        }
                    }
                }
            }

            // 桃华定制：保存安装路径到标记文件，供未来安装器恢复使用
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(install_dir) = exe_path.parent() {
                    let _ = fs::write(
                        data_dir.join(".install_path"),
                        install_dir.to_string_lossy().as_ref(),
                    );
                }
            }

            // ── 系统托盘 ─────────────────────
            let open_item =
                MenuItemBuilder::with_id("open", "打开 Minagi 文件分类助手").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出软件").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .item(&open_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // 桃华定制：用 with_id 给 tray 图标一个固定的标识符
            // Windows 11 任务栏角溢出设置靠 GUID 匹配图标偏好
            // 随机 GUID 会让每次启动时 Windows 忘记用户的"显示"设置
            // 用固定的 id 能让 Tauri 生成稳定的 GUID
            let _tray = TrayIconBuilder::with_id("minagi-file-classifier")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Minagi 文件分类助手")
                .menu(&tray_menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "open" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键单击托盘图标 → 显示并聚焦窗口
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            // 通知前端重置 hover 状态（WebView2 恢复窗口时 :hover 会粘住）
                            let _ = app.emit("window-restored", ());
                        }
                    }
                })
                .build(app)?;

            // ── 窗口事件：关闭拦截 + 原生拖放 ─────
            if let Some(window) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                window.on_window_event(move |event| {
                    // Alt+F4 / 任务栏关闭 → 隐藏到托盘
                    if let tauri::WindowEvent::CloseRequested { api, .. } = &event {
                        let close_to_tray = app_handle
                            .try_state::<ConfigState>()
                            .and_then(|s| s.0.lock().ok().map(|c| c.close_to_tray))
                            .unwrap_or(true);
                        if close_to_tray {
                            api.prevent_close();
                            // 通知前端清除按钮 hover 状态
                            let _ = app_handle.emit("window-hiding", ());
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.hide();
                            }
                        }
                        return;
                    }

                    // 原生文件拖放 → 发射到前端（WebView2 没有 file.path）
                    if let tauri::WindowEvent::DragDrop(dd) = &event {
                        match dd {
                            tauri::DragDropEvent::Enter { paths, position } => {
                                let _ = app_handle.emit("tauri-drag-enter", serde_json::json!({
                                    "count": paths.len(),
                                    "x": position.x,
                                    "y": position.y,
                                }));
                            }
                            tauri::DragDropEvent::Over { position } => {
                                let _ = app_handle.emit("tauri-drag-over", serde_json::json!({
                                    "x": position.x,
                                    "y": position.y,
                                }));
                            }
                            tauri::DragDropEvent::Drop { paths, position } => {
                                let paths_str: Vec<String> = paths
                                    .iter()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .collect();
                                let _ = app_handle.emit("tauri-drop", serde_json::json!({
                                    "paths": paths_str,
                                    "x": position.x,
                                    "y": position.y,
                                }));
                            }
                            tauri::DragDropEvent::Leave => {
                                let _ = app_handle.emit("tauri-drag-leave", serde_json::json!({}));
                            }
                            _ => {}
                        }
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("启动失败……桃华检查一下配置？");
}
