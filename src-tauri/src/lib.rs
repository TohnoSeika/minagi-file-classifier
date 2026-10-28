/// Minagi 文件分类助手 v2.0 — Tauri 版 主进程
/// 这个文件是桃华帮 Minagi 从 Electron 迁移到 Tauri 的哦 (*´▽`*)
///
/// 和 Electron 版相比：
///   - 后端从 Node.js 换成了 Rust，文件操作更快更安全
///   - 不再打包 Chromium，体积从 366MB 降到 ~10MB
///   - 内存占用从 ~200MB 降到 ~50MB

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

// ─── 应用状态 ─────────────────────────────────────

/// 取消令牌 —— 文件操作中的取消标记
struct CancelState(Arc<AtomicBool>);

/// 任务锁 —— 避免多个复制/剪切任务互相踩进度
struct TaskState(Arc<AtomicBool>);

/// 任务标志的守卫：不管正常结束、提前返回还是 panic，
/// 离开作用域都会自动复位。以前靠每个分支手工 store(false)，
/// 漏掉一处就会一直报「正在处理上一批文件」
struct RunningGuard(Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// 恢复锁 —— 同一时间只允许一个快照恢复任务
struct RestoreState(Mutex<()>);

/// 配置缓存 —— 避免频繁读文件
struct ConfigState(Mutex<Config>);

/// 操作历史 —— 最多 50 条
struct HistoryState(Mutex<Vec<HistoryEntry>>);

/// 历史文件锁 —— 「追加一条」和「清空」不许交错
/// 桃华修复：以前读-改-写没有锁，清空的同时刚好完成一批，
/// 旧记录会被整份写回来
struct HistoryFileLock(Mutex<()>);

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

/// 递归遍历的深度上限
/// 桃华修复：junction / 符号链接可以绕成环，没有上限的话会一直往下走
const MAX_WALK_DEPTH: usize = 64;

/// 记下一个目录的规范路径，已经走过就返回 false
/// 链接指回已经访问过的目录时，靠它把循环截断
fn mark_visited(path: &Path, visited: &mut HashSet<PathBuf>) -> bool {
    let key = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    visited.insert(key)
}

/// 递归统计文件/文件夹内的总字节数
fn count_bytes(path: &Path) -> u64 {
    let mut visited = HashSet::new();
    count_bytes_guarded(path, 0, &mut visited)
}

fn count_bytes_guarded(path: &Path, depth: usize, visited: &mut HashSet<PathBuf>) -> u64 {
    if depth > MAX_WALK_DEPTH {
        return 0;
    }

    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => {
            if !mark_visited(path, visited) {
                return 0;
            }
            fs::read_dir(path)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .map(|e| count_bytes_guarded(&e.path(), depth + 1, visited))
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

/// 删除文件或目录
/// 桃华注：Windows 上删文件和删目录是两个 API，不能靠「两个都试一遍」碰运气
fn remove_path(path: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// 桃华加固：先写临时文件再改名替换
/// 直接 fs::write 会先截断再写，写到一半断电/被杀进程就只剩半截 JSON，
/// 下次启动连配置都读不出来
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    // 临时文件名带序号：两次保存撞在一起也不会写同一个文件
    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_extension(format!("json.tmp{}", seq));

    fs::write(&temp, contents)?;

    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(e);
    }

    Ok(())
}

/// 目标是否落在源目录内部（含相等）
/// 桃华修复：这种情况下复制会把刚建出来的目标目录再枚举一遍，一层层套娃
/// 用规范路径比较，junction 和符号链接也能一起解开
fn target_is_inside(source: &Path, target: &Path) -> bool {
    let Ok(source) = fs::canonicalize(source) else {
        return false;
    };
    let Ok(target) = fs::canonicalize(target) else {
        return false;
    };
    target == source || target.starts_with(&source)
}

/// 找最新一份能读出来的快照配置
/// 桃华加固：config.json 损坏时用它兜底，总比界面空着好
fn latest_valid_config(data_dir: &Path) -> Option<Config> {
    let snapshots_dir = data_dir.join(SNAPSHOTS_DIR);

    let mut snapshots: Vec<(u64, PathBuf)> = fs::read_dir(&snapshots_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            Some((snapshot_seconds(&path)?, path))
        })
        .collect();

    snapshots.sort_by(|a, b| b.0.cmp(&a.0));

    snapshots.into_iter().find_map(|(_, path)| {
        fs::read_to_string(path.join("config.json"))
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    })
}

/// 把逐项失败汇总成一句能塞进 toast 的话
fn format_warning_summary(warnings: &[String]) -> String {
    const MAX_SHOWN: usize = 3;

    let shown = warnings
        .iter()
        .take(MAX_SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join("；");

    if warnings.len() > MAX_SHOWN {
        format!("{}；还有 {} 个没成功", shown, warnings.len() - MAX_SHOWN)
    } else {
        shown
    }
}

// ═══════════════════════════════════════════════════
// 桃华定制：配置快照机制
// 每次用户改区域名/路径/背景图时自动存档，最多保留 MAX_SNAPSHOTS 份
// ═══════════════════════════════════════════════════

/// 快照目录名就是 Unix 秒数，不是纯数字的旧目录一律不认
fn snapshot_seconds(path: &Path) -> Option<u64> {
    path.file_name()?.to_str()?.parse().ok()
}

/// 找出最新的一份快照
fn latest_snapshot(snapshots_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(snapshots_dir).ok()?;

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| snapshot_seconds(&path).map(|ts| (ts, path)))
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, path)| path)
}

/// 创建一份配置快照（config.json + 当前所有背景图片）
/// 目录名为 Unix 时间戳，前端用 JS Date 格式化为本地时间
fn create_snapshot(data_dir: &Path) {
    let config_file = data_dir.join("config.json");
    let images_dir = data_dir.join("images");
    let snapshots_dir = data_dir.join(SNAPSHOTS_DIR);

    let Ok(current_config) = fs::read_to_string(&config_file) else {
        return;
    };

    // 桃华补充：配置没变就不重新存档，不然同一份配置能把所有位置都占满
    if let Some(latest) = latest_snapshot(&snapshots_dir) {
        let unchanged = fs::read_to_string(latest.join("config.json"))
            .map(|previous| previous == current_config)
            .unwrap_or(false);
        if unchanged {
            return;
        }
    }

    // 同一秒内保存多次时往后顺延，保证目录名唯一（名字必须是数字）
    let mut ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut snap_dir = snapshots_dir.join(ts.to_string());
    while snap_dir.exists() {
        ts += 1;
        snap_dir = snapshots_dir.join(ts.to_string());
    }

    let _ = fs::create_dir_all(&snap_dir);
    let _ = fs::write(snap_dir.join("config.json"), &current_config);

    if images_dir.exists() {
        let snap_images = snap_dir.join("images");
        let _ = fs::create_dir_all(&snap_images);
        // 快照只是尽力而为，失败不影响主流程
        let _ = link_or_copy_dir(&images_dir, &snap_images);
    }

    cleanup_snapshots(&snapshots_dir);
}

/// 递归复制目录内容
/// 桃华修复：错误交回给调用者。快照恢复要靠它判断有没有复制完整，
/// 以前全部 `let _` 吞掉，失败了也当成功
fn copy_dir_content(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());

        if path.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_dir_content(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// 桃华优化：快照图片用硬链接指回当前图片，同一份数据不再复制七遍。
/// 硬链接不成功（极少数文件系统）就老老实实复制
fn link_or_copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());

        if path.is_dir() {
            link_or_copy_dir(&path, &dest)?;
        } else if fs::hard_link(&path, &dest).is_err() {
            fs::copy(&path, &dest)?;
        }
    }

    Ok(())
}

/// 把配置里引用到的图片文件名收集起来
fn collect_image_refs(config: &Config, into: &mut HashSet<String>) {
    let backgrounds = std::iter::once(&config.global_bg)
        .chain(config.zones.iter().map(|zone| &zone.bg));

    for background in backgrounds {
        if let Some(name) = Path::new(background).file_name() {
            into.insert(name.to_string_lossy().to_string());
        }
    }
}

/// 清理不再被任何配置（当前 + 所有快照）引用的背景图
/// 桃华补充：以前换一次背景就留一个文件，磁盘只增不减
fn cleanup_orphan_images(data_dir: &Path, current: &Config) {
    let images_dir = data_dir.join("images");
    let Ok(entries) = fs::read_dir(&images_dir) else {
        return;
    };

    let mut keep = HashSet::new();
    collect_image_refs(current, &mut keep);

    // 快照里还引用着的图片不能删，不然恢复出来会缺图
    if let Ok(snapshots) = fs::read_dir(data_dir.join(SNAPSHOTS_DIR)) {
        for entry in snapshots.flatten() {
            let config_path = entry.path().join("config.json");
            let Ok(text) = fs::read_to_string(config_path) else {
                continue;
            };
            if let Ok(config) = serde_json::from_str::<Config>(&text) {
                collect_image_refs(&config, &mut keep);
            }
        }
    }

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !keep.contains(&name) {
            let _ = fs::remove_file(&path);
        }
    }
}

// ── 桃华注：AppData 共用 minagi-file-classifier，无需迁移 ──

/// 保留最近 MAX_SNAPSHOTS 份，删除更旧的
fn cleanup_snapshots(snapshots_dir: &Path) {
    let Ok(entries) = fs::read_dir(snapshots_dir) else {
        return;
    };

    let mut snapshots: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && snapshot_seconds(path).is_some())
        .collect();

    // 新的排前面，超出的部分删掉
    snapshots.sort_by_key(|path| std::cmp::Reverse(snapshot_seconds(path).unwrap_or(0)));

    for old in snapshots.iter().skip(MAX_SNAPSHOTS) {
        let _ = fs::remove_dir_all(old);
    }
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
    let mut visited = HashSet::new();
    copy_folder_guarded(source, dest, on_bytes, cancelled, 0, &mut visited)
}

/// 桃华修复：和 count_bytes 一样防链接环。
/// 太深或者绕回已经访问过的目录就跳过这一支，不跟着无限复制
fn copy_folder_guarded(
    source: &Path,
    dest: &Path,
    on_bytes: &mut dyn FnMut(u64),
    cancelled: &AtomicBool,
    depth: usize,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("cancelled".to_string());
    }
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }

    let meta = fs::metadata(source).map_err(|e| format!("无法访问: {}", e))?;

    if meta.is_dir() {
        if !mark_visited(source, visited) {
            return Ok(());
        }
        fs::create_dir_all(dest).map_err(|e| format!("无法创建目录: {}", e))?;

        let entries = fs::read_dir(source).map_err(|e| format!("无法读取目录: {}", e))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            copy_folder_guarded(
                &entry.path(),
                &dest.join(entry.file_name()),
                on_bytes,
                cancelled,
                depth + 1,
                visited,
            )?;
        }
    } else {
        copy_file_with_progress(source, dest, on_bytes, cancelled)?;
    }

    Ok(())
}

// ─── 文件移动/复制核心逻辑 ─────────────────────

/// 在后台执行文件移动/复制，通过事件系统推送进度
/// 桃华注：这里是同步阻塞 IO，调用方负责把它放到阻塞线程池里
fn do_move_files(
    app_handle: tauri::AppHandle,
    file_paths: Vec<String>,
    target_dir: String,
    mode: String,
    cancelled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
) {
    // 桃华修复：任务标志交给 Drop 复位，panic 也不会把软件卡在「忙」状态
    let _running_guard = RunningGuard(running);

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
                warning: None,
                count: None,
            },
        );
        return;
    }

    // 桃华修复：目标在源目录里面时，复制会把刚建出来的目标再枚举一遍，
    // 一层层套娃直到盘满。动手前先算清楚，别等写满了才发现
    for source_path in &file_paths {
        if target_is_inside(Path::new(source_path), target) {
            let _ = app_handle.emit(
                "task-progress",
                ProgressPayload {
                    status: "error".into(),
                    total: None,
                    percent: None,
                    current: None,
                    error: Some("不能把文件夹放进它自己里面呢……换一个目标试试".into()),
                    warning: None,
                    count: None,
                },
            );
            return;
        }
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
            warning: None,
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
                    warning: None,
                    count: None,
                },
            );
            last_send = now;
        }
    };

    // 桃华修复：一个文件失败不再中断整批，记下来继续处理剩下的
    let mut done_paths: Vec<String> = Vec::new();
    let mut done_names: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for source_path_str in &file_paths {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let source = Path::new(source_path_str);
        let base_name = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dest_path = get_unique_path(&target.join(&base_name));

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
                // 桃华修复：取消或失败时把目标位置的半截文件清掉
                // 以前取消之后目标文件夹里会留一堆不完整的东西
                let _ = remove_path(&dest_path);

                if e == "cancelled" {
                    break;
                }

                // 一个文件失败不影响后面的，最后一起汇报
                warnings.push(format!("「{}」没能处理：{}", base_name, e));
                continue;
            }

            // 剪切模式下拷贝完成后删除源文件
            if mode == "cut" && !cancelled.load(Ordering::Relaxed) {
                // 桃华修复：删除失败要告诉用户，不能让人以为剪切成功
                if let Err(e) = remove_path(source) {
                    warnings.push(format!(
                        "「{}」已复制，但源文件删除失败：{}",
                        base_name, e
                    ));
                }
            }
        }

        done_paths.push(source_path_str.clone());
        done_names.push(base_name);
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
                warning: None,
                count: None,
            },
        );
    } else {
        // ── 只把真正处理成功的项目记进历史 ──
        let success_count = done_paths.len();

        if success_count > 0 {
            let entry = HistoryEntry {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                source_paths: done_paths,
                target_dir: target_dir.clone(),
                mode: mode.clone(),
                file_names: done_names,
                item_count: success_count,
            };
            save_history(&app_handle, entry);
        }

        let _ = app_handle.emit(
            "task-progress",
            ProgressPayload {
                status: "end".into(),
                total: None,
                percent: None,
                current: None,
                error: None,
                warning: if warnings.is_empty() {
                    None
                } else {
                    Some(format_warning_summary(&warnings))
                },
                count: Some(success_count),
            },
        );
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
        // 桃华加固：读不出来或者坏了都别让界面空着，先找最新一份能读的快照
        fs::read_to_string(&config_file)
            .ok()
            .and_then(|content| serde_json::from_str::<Config>(&content).ok())
            .unwrap_or_else(|| {
                latest_valid_config(&config_dir.join("MinagiData")).unwrap_or_default()
            })
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
    let snapshot = do_snapshot.unwrap_or(false);

    // 桃华修复：存配置会顺手创建快照、复制图片，都是阻塞 IO，
    // 不能占着 async 运行时，交给阻塞线程池
    tauri::async_runtime::spawn_blocking(move || {
        save_config_blocking(&app_handle, config, snapshot)
    })
    .await
    .map_err(|e| format!("保存任务异常退出: {}", e))?
}

fn save_config_blocking(
    app_handle: &tauri::AppHandle,
    config: Config,
    do_snapshot: bool,
) -> Result<(), String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let config_file = data_dir.join("config.json");
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    write_atomic(&config_file, &json).map_err(|e| e.to_string())?;

    // 更新缓存
    if let Some(state) = app_handle.try_state::<ConfigState>() {
        if let Ok(mut cached) = state.0.lock() {
            *cached = config.clone();
        }
    }

    // 区域名/路径/背景图变更时创建快照
    if do_snapshot {
        create_snapshot(&data_dir);
    }

    // 桃华补充：顺手清掉已经没人引用的旧背景图
    cleanup_orphan_images(&data_dir, &config);

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
    // 桃华修复：拷贝是同步阻塞 IO，放到阻塞线程池，别占着 async 运行时
    let handle = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        do_move_files(handle, file_paths, target_dir, mode, cancel_flag, running);
    });

    Ok(())
}

#[tauri::command]
async fn cancel_move(app_handle: tauri::AppHandle) -> Result<(), String> {
    let state = app_handle.state::<CancelState>();
    state.0.store(true, Ordering::SeqCst);
    Ok(())
}

fn read_history_file(path: &Path) -> Vec<HistoryEntry> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// 保存历史记录到磁盘（最多 50 条）
fn save_history(app_handle: &tauri::AppHandle, entry: HistoryEntry) {
    let Ok(config_dir) = app_handle.path().app_data_dir() else {
        return;
    };
    let data_dir = config_dir.join("MinagiData");
    let history_file = data_dir.join("history.json");
    let _ = fs::create_dir_all(&data_dir);

    // 桃华修复：读-改-写整段加锁，避免和「清空」互相覆盖
    let Some(lock) = app_handle.try_state::<HistoryFileLock>() else {
        return;
    };
    let Ok(_guard) = lock.0.lock() else {
        return;
    };

    let mut history = read_history_file(&history_file);
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

    // 桃华修复：原子替换，半截 JSON 不会把整份历史写坏
    let _ = write_atomic(&history_file, &serde_json::to_string_pretty(&history).unwrap_or_default());
}

#[tauri::command]
async fn load_history(app_handle: tauri::AppHandle) -> Result<Vec<HistoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || load_history_blocking(&app_handle))
        .await
        .map_err(|e| format!("读取历史任务异常退出: {}", e))?
}

fn load_history_blocking(app_handle: &tauri::AppHandle) -> Result<Vec<HistoryEntry>, String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let history_file = config_dir.join("MinagiData").join("history.json");

    if !history_file.exists() {
        return Ok(Vec::new());
    }

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
}

#[tauri::command]
async fn clear_history(app_handle: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || clear_history_blocking(&app_handle))
        .await
        .map_err(|e| format!("清空历史任务异常退出: {}", e))?
}

fn clear_history_blocking(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let history_file = data_dir.join("history.json");

    // 和 save_history 用同一把锁
    let lock = app_handle
        .try_state::<HistoryFileLock>()
        .ok_or_else(|| "历史记录锁没有初始化".to_string())?;
    let _guard = lock.0.lock().map_err(|_| "历史记录锁坏了".to_string())?;

    write_atomic(&history_file, "[]").map_err(|e| e.to_string())?;

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
    tauri::async_runtime::spawn_blocking(move || list_snapshots_blocking(&app_handle))
        .await
        .map_err(|e| format!("读取快照列表任务异常退出: {}", e))?
}

fn list_snapshots_blocking(app_handle: &tauri::AppHandle) -> Result<Vec<SnapshotInfo>, String> {
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
    tauri::async_runtime::spawn_blocking(move || {
        restore_snapshot_blocking(&app_handle, &snapshot_name)
    })
    .await
    .map_err(|e| format!("恢复任务异常退出: {}", e))?
}

fn restore_snapshot_blocking(
    app_handle: &tauri::AppHandle,
    snapshot_name: &str,
) -> Result<(), String> {
    // 桃华加固：快照名只能是纯数字，挡住 ../ 这类路径穿越
    if snapshot_name.is_empty() || !snapshot_name.bytes().all(|b| b.is_ascii_digit()) {
        return Err("快照名称不合法".into());
    }

    // 桃华加固：同一时间只允许一个恢复任务，双击也不会互相删目录
    let restore_state = app_handle.state::<RestoreState>();
    let _restore_guard = restore_state
        .0
        .try_lock()
        .map_err(|_| "正在恢复上一份快照，等一下……".to_string())?;

    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_dir = config_dir.join("MinagiData");
    let snap_dir = data_dir.join(SNAPSHOTS_DIR).join(snapshot_name);

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

    // 桃华修复：先把快照图片复制到临时目录。
    // 这一步要放在 create_snapshot 之前——新快照可能把最旧的那份挤掉，
    // 要恢复的恰好是它的话，后面就找不到图片了
    let snap_images = snap_dir.join("images");
    let images_dir = data_dir.join("images");
    let staging = data_dir.join("images.restore");
    let _ = fs::remove_dir_all(&staging);

    if snap_images.exists() {
        fs::create_dir_all(&staging).map_err(|e| format!("无法创建临时目录: {}", e))?;
        copy_dir_content(&snap_images, &staging)
            .map_err(|e| format!("复制快照图片失败: {}", e))?;
    }

    // 桃华修复：恢复前先给当前状态存一份，出问题还能退回来
    create_snapshot(&data_dir);

    // 图片目录先换，config 最后写。任何一步失败都尽量还原
    let backup = data_dir.join("images.before-restore");
    let _ = fs::remove_dir_all(&backup);

    let had_images = images_dir.exists();
    if had_images {
        fs::rename(&images_dir, &backup)
            .map_err(|e| format!("无法备份当前图片: {}", e))?;
    }

    if snap_images.exists() {
        if let Err(e) = fs::rename(&staging, &images_dir) {
            if had_images {
                let _ = fs::rename(&backup, &images_dir);
            }
            return Err(format!("无法替换图片目录: {}", e));
        }
    }

    if let Err(e) = write_atomic(&data_dir.join("config.json"), &content) {
        // 配置没写成功，把图片换回去
        let _ = fs::remove_dir_all(&images_dir);
        if had_images {
            let _ = fs::rename(&backup, &images_dir);
        }
        return Err(format!("无法写入配置: {}", e));
    }

    let _ = fs::remove_dir_all(&backup);
    let _ = fs::remove_dir_all(&staging);

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

/// 根据文件头判断图片格式
/// 桃华修复：以前按原文件名取扩展名，前端压缩后明明是 JPEG，
/// 却会存成 .png，文件内容和后缀对不上
fn detect_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"BM") {
        Some("bmp")
    } else {
        None
    }
}

#[tauri::command]
async fn import_image(
    app_handle: tauri::AppHandle,
    file_data_b64: String,
) -> Result<String, String> {
    use base64::Engine;
    const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

    if file_data_b64.is_empty() {
        return Ok(String::new());
    }

    // 桃华修复：先按 base64 长度估一下，明显超限就别解码了。
    // 以前先解码再检查，超大字符串会先占满一块内存
    const MAX_B64_BYTES: usize = (MAX_IMAGE_BYTES / 3 + 1) * 4;
    if file_data_b64.len() > MAX_B64_BYTES {
        return Err("图片太大了，换一张 10MB 以内的吧".into());
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&file_data_b64)
        .map_err(|e| format!("解码失败: {}", e))?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("图片太大了，换一张 10MB 以内的吧".into());
    }

    let ext = detect_image_extension(&bytes).ok_or("只支持常见图片格式")?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let config_dir = app_handle.path().app_data_dir().map_err(|e| e.to_string())?;
    let images_dir = config_dir.join("MinagiData").join("images");
    fs::create_dir_all(&images_dir).map_err(|e| e.to_string())?;

    let dest = get_unique_path(&images_dir.join(format!("img_{}.{}", timestamp, ext)));
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

#[tauri::command]
async fn open_external(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("无法打开链接: {}", e))
}

// ═══════════════════════════════════════════════════
// 桃华定制：旧版配置迁移
// 全程用 serde_json::Value 操作，不反序列化成 Config 再写回，
// 这样旧配置里不认识的字段不会在迁移时悄悄丢掉
// ═══════════════════════════════════════════════════

fn migrate_legacy_config(config_file: &Path, images_dir: &Path, data_dir: &Path) {
    let Ok(content) = fs::read_to_string(config_file) else {
        return;
    };
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };
    let Some(obj) = root.as_object_mut() else {
        return;
    };

    let mut changed = false;

    // ── 第一步：旧字段名 bg / backgroundImage / bgImage → globalBg / bg ──
    let has_global_bg = obj
        .get("globalBg")
        .and_then(|value| value.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if !has_global_bg {
        for old_key in ["bg", "backgroundImage", "bgImage"] {
            let Some(value) = obj.remove(old_key) else {
                continue;
            };
            if value.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                obj.insert("globalBg".to_string(), value);
                changed = true;
                break;
            }
        }
    }

    if let Some(zones) = obj.get_mut("zones").and_then(|value| value.as_array_mut()) {
        for zone in zones {
            let Some(zone_obj) = zone.as_object_mut() else {
                continue;
            };
            let has_bg = zone_obj
                .get("bg")
                .and_then(|value| value.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_bg {
                continue;
            }
            let Some(value) = zone_obj.remove("backgroundImage") else {
                continue;
            };
            if value.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                zone_obj.insert("bg".to_string(), value);
                changed = true;
            }
        }
    }

    // ── 第二步：修复背景图路径 ──
    if let Some(value) = obj.get_mut("globalBg") {
        changed |= repair_bg_value(value, images_dir, data_dir);
    }
    if let Some(zones) = obj.get_mut("zones").and_then(|value| value.as_array_mut()) {
        for zone in zones {
            if let Some(value) = zone.get_mut("bg") {
                changed |= repair_bg_value(value, images_dir, data_dir);
            }
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string_pretty(&root) {
            let _ = write_atomic(config_file, &json);
        }
    }
}

/// 把一条背景图配置修到「界面真的能加载」的状态
/// data URL → 存成文件；路径失效 → 在 images 里找同名文件；
/// 文件在 asset scope 外 → 复制进 images
fn repair_bg_value(
    value: &mut serde_json::Value,
    images_dir: &Path,
    data_dir: &Path,
) -> bool {
    let Some(raw) = value.as_str().map(str::to_string) else {
        return false;
    };
    if raw.is_empty() {
        return false;
    }

    if raw.starts_with("data:") {
        if let Some(saved) = save_data_url_image(&raw, images_dir) {
            *value = serde_json::Value::String(saved);
            return true;
        }
        return false;
    }

    let path = Path::new(&raw);

    if !path.exists() {
        if let Some(name) = path.file_name() {
            let candidate = images_dir.join(name);
            if candidate.exists() {
                *value = serde_json::Value::String(to_asset_path(&candidate));
                return true;
            }
        }
        return false;
    }

    // 文件存在，但不在 asset scope 里的话界面加载不出来，复制进来
    if is_inside_data_dir(path, data_dir) {
        return false;
    }
    if let Some(saved) = copy_image_into(&raw, images_dir) {
        *value = serde_json::Value::String(saved);
        return true;
    }

    false
}

fn to_asset_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_inside_data_dir(path: &Path, data_dir: &Path) -> bool {
    let Ok(dir) = fs::canonicalize(data_dir) else {
        return true;
    };
    let Ok(file) = fs::canonicalize(path) else {
        return true;
    };
    file.starts_with(dir)
}

fn save_data_url_image(raw: &str, images_dir: &Path) -> Option<String> {
    use base64::Engine;

    let (meta, payload) = raw.strip_prefix("data:")?.split_once(',')?;
    if !meta.contains(";base64") {
        return None;
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()?;
    let ext = detect_image_extension(&bytes)?;

    fs::create_dir_all(images_dir).ok()?;

    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let dest = get_unique_path(&images_dir.join(format!("img_migrated_{}.{}", millis, ext)));
    fs::write(&dest, bytes).ok()?;

    Some(to_asset_path(&dest))
}

fn copy_image_into(source: &str, images_dir: &Path) -> Option<String> {
    let source_path = Path::new(source);
    let name = source_path.file_name()?;

    fs::create_dir_all(images_dir).ok()?;

    let dest = get_unique_path(&images_dir.join(name));
    fs::copy(source_path, &dest).ok()?;

    Some(to_asset_path(&dest))
}

/// 桃华补充：小屏幕 / 150% 缩放下 780×550 可能比可用区域还高，
/// 启动时按显示器尺寸收一收，别让底下的区域被切掉
fn fit_window_to_screen(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    let scale = monitor.scale_factor();
    let logical_width = monitor.size().width as f64 / scale;
    let logical_height = monitor.size().height as f64 / scale;

    // 给任务栏和窗口留白留一点空间
    let max_width = (logical_width - 40.0).max(640.0);
    let max_height = (logical_height - 90.0).max(420.0);

    let width = 780.0_f64.min(max_width);
    let height = 550.0_f64.min(max_height);

    if width < 780.0 || height < 550.0 {
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        let _ = window.center();
    }
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
        .manage(RestoreState(Mutex::new(())))
        .manage(ConfigState(Mutex::new(default_config)))
        .manage(HistoryState(Mutex::new(Vec::new())))
        .manage(HistoryFileLock(Mutex::new(())))
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            move_files,
            cancel_move,
            open_folder,
            import_image,
            load_history,
            clear_history,
            win_minimize,
            win_close,
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
                            let _ = write_atomic(&cfg_path, &json);
                        }
                    }
                }
                create_snapshot(&data_dir);
                let _ = fs::write(&first_snap_marker, "1");
            }

            // 桃华定制：验证并修复背景图路径 + 迁移旧版配置
            // 桃华修复：整段交给 migrate_legacy_config，全程操作 Value，
            // 不再反序列化成 Config 再写回，旧字段不会丢
            let config_file = data_dir.join("config.json");
            if config_file.exists() {
                migrate_legacy_config(&config_file, &images_dir, &data_dir);
            }

            // 桃华补充：小屏幕 / 高缩放下先把窗口收进可用区域
            fit_window_to_screen(app);

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
            //
            // 桃华修复：以前 default_window_icon().unwrap()，图标缺失直接 panic，
            // 连主窗口都起不来。现在没有图标就跳过托盘，创建失败也只记录
            let tray_result = app.default_window_icon().cloned().map(|icon| {
                TrayIconBuilder::with_id("minagi-file-classifier")
                    .icon(icon)
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
                    .build(app)
            });

            if let Some(Err(e)) = tray_result {
                eprintln!("托盘图标创建失败：{}", e);
            }

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
                    // 桃华修复：DragDropEvent 的 position 是物理像素坐标，
                    // 但前端 elementFromPoint() 需要逻辑像素坐标，
                    // 需要除以缩放因子再做转换
                    if let tauri::WindowEvent::DragDrop(dd) = &event {
                        // 获取当前缩放因子
                        let scale = app_handle
                            .get_webview_window("main")
                            .and_then(|w| w.scale_factor().ok())
                            .unwrap_or(1.0);
                        let to_logical = |pos: &tauri::PhysicalPosition<f64>| -> (f64, f64) {
                            (pos.x / scale, pos.y / scale)
                        };

                        match dd {
                            tauri::DragDropEvent::Enter { paths, position } => {
                                let (lx, ly) = to_logical(position);
                                let _ = app_handle.emit("tauri-drag-enter", serde_json::json!({
                                    "count": paths.len(),
                                    "x": lx,
                                    "y": ly,
                                }));
                            }
                            tauri::DragDropEvent::Over { position } => {
                                let (lx, ly) = to_logical(position);
                                let _ = app_handle.emit("tauri-drag-over", serde_json::json!({
                                    "x": lx,
                                    "y": ly,
                                }));
                            }
                            tauri::DragDropEvent::Drop { paths, position } => {
                                let (lx, ly) = to_logical(position);
                                let paths_str: Vec<String> = paths
                                    .iter()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .collect();
                                let _ = app_handle.emit("tauri-drop", serde_json::json!({
                                    "paths": paths_str,
                                    "x": lx,
                                    "y": ly,
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

// ─── 测试 ─────────────────────────────────────────
// 这几个保护逻辑一旦写错就是磁盘/配置出事故，桃华想先钉住它们
#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "minagi-test-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn target_inside_source_is_rejected() {
        let root = scratch_dir("inside");
        let source = root.join("Downloads");
        let nested = source.join("分类");
        let sibling = root.join("Pictures");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        assert!(target_is_inside(&source, &nested));
        assert!(target_is_inside(&source, &source));
        assert!(!target_is_inside(&source, &sibling));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_atomic_replaces_file_without_leftovers() {
        let root = scratch_dir("atomic");
        let file = root.join("config.json");

        write_atomic(&file, r#"{"opacity":0.5}"#).unwrap();
        write_atomic(&file, r#"{"opacity":0.8}"#).unwrap();

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("0.8"));

        let files = fs::read_dir(&root).unwrap().flatten().count();
        assert_eq!(files, 1, "临时文件改名后不该留下");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn latest_valid_config_skips_broken_snapshots() {
        let root = scratch_dir("fallback");
        let snapshots = root.join(SNAPSHOTS_DIR);

        for (name, body) in [("100", r#"{"opacity":0.4}"#), ("200", "{ broken")] {
            let dir = snapshots.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("config.json"), body).unwrap();
        }

        let config = latest_valid_config(&root).expect("应该回退到能读的快照");
        assert!((config.opacity - 0.4).abs() < f64::EPSILON);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn warning_summary_caps_long_lists() {
        let warnings: Vec<String> = (0..5).map(|i| format!("第 {} 个失败", i)).collect();
        let summary = format_warning_summary(&warnings);
        assert!(summary.contains("还有 2 个没成功"));
    }

    #[test]
    fn migration_keeps_unknown_fields() {
        let root = scratch_dir("migrate");
        let images = root.join("images");
        fs::create_dir_all(&images).unwrap();

        let config_file = root.join("config.json");
        let legacy = r#"{
            "opacity": 0.8,
            "bg": "C:/old/picture.png",
            "customField": { "keep": 42 },
            "zones": []
        }"#;
        fs::write(&config_file, legacy).unwrap();

        migrate_legacy_config(&config_file, &images, &root);

        let text = fs::read_to_string(&config_file).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(saved["globalBg"], "C:/old/picture.png");
        assert_eq!(saved["customField"]["keep"], 42, "不认识的字段不能在迁移时丢掉");
        assert!(saved.get("bg").is_none(), "旧字段迁移后应该被移除");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphan_images_are_cleaned_but_referenced_ones_stay() {
        let root = scratch_dir("images");
        let images = root.join("images");
        fs::create_dir_all(&images).unwrap();
        for name in ["now.jpg", "snapshot.jpg", "orphan.jpg"] {
            fs::write(images.join(name), b"x").unwrap();
        }

        let snapshot_dir = root.join(SNAPSHOTS_DIR).join("100");
        fs::create_dir_all(&snapshot_dir).unwrap();
        let mut snapshot = Config::default();
        snapshot.global_bg = to_asset_path(&images.join("snapshot.jpg"));
        fs::write(
            snapshot_dir.join("config.json"),
            serde_json::to_string(&snapshot).unwrap(),
        )
        .unwrap();

        let mut current = Config::default();
        current.global_bg = to_asset_path(&images.join("now.jpg"));
        cleanup_orphan_images(&root, &current);

        assert!(images.join("now.jpg").exists());
        assert!(images.join("snapshot.jpg").exists(), "快照里引用的图不能被删");
        assert!(!images.join("orphan.jpg").exists(), "没人引用的图应该被清掉");

        let _ = fs::remove_dir_all(&root);
    }
}
