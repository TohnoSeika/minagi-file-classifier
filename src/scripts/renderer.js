/**
 * Minagi 文件分类助手 v2.0.1 — 渲染进程（Tauri 版）
 * 桃华把 UI 逻辑整理成了几个清晰的小模块 ✨
 * 和 Electron 版不同的地方：通过 Tauri invoke 桥接，更轻更快
 */

// ─── Tauri API 桥接层 ─────────────────────────────
// 桃华把 Electron 的 preload 桥换成了 Tauri 的 invoke ✨
// window.__TAURI__ 在 withGlobalTauri: true 时由 Tauri 注入

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

window.sorterAPI = {
    loadConfig:   ()           => invoke('load_config'),
    saveConfig:   (config, doSnapshot = false) => invoke('save_config', { config, doSnapshot }),
    listSnapshots: ()           => invoke('list_snapshots'),
    restoreSnapshot: (name)     => invoke('restore_snapshot', { snapshotName: name }),
    moveFiles:    (payload)    => invoke('move_files', {
                                    filePaths: payload.filePaths,
                                    targetDir: payload.targetDir,
                                    mode: payload.mode,
                                 }),
    cancelMove:   ()           => invoke('cancel_move'),
    openFolder:   (targetPath) => invoke('open_folder', { path: targetPath }),
    importImage:       (fileName, fileDataB64) => invoke('import_image', { fileName, fileDataB64 }),
    readImageDataUrl:  (path)       => invoke('read_image_data_url', { path }),
    loadHistory:       ()           => invoke('load_history'),
    clearHistory:      ()           => invoke('clear_history'),
    winMinimize:       ()           => invoke('win_minimize'),
    winMaximize:  ()           => invoke('win_maximize'),
    winClose:     ()           => invoke('win_close'),
    winMove:      (delta)      => invoke('win_move', { dx: delta.dx, dy: delta.dy }),
    setOpacity:   (val)        => invoke('set_opacity', { opacity: val }),
    openExternal: (url)        => invoke('open_external', { url }),
    onProgress:   (callback)   => { listen('task-progress', (event) => callback(event.payload)); },
};

// ─── Tauri 图片路径转换 ─────────────────────────
// WebView2 不允许直接加载 file:// URL
// 需要用 convertFileSrc 转成 Tauri asset 协议
/** 用 FileReader 读取文件为 data URL */
function readFileAsDataUrl(file) {
    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
    });
}

/**
 * 压缩图片以避免通过 WebView2 IPC 传输过大的 base64
 * WebView2 的 postMessage 对大消息（>1MB）可能挂起
 * 这里将图片缩放到最大 1920px 并用 JPEG 0.8 质量压缩
 */
async function compressImageForIpc(file, maxDim = 1920, quality = 0.8) {
    const bitmap = await createImageBitmap(file);
    let w = bitmap.width, h = bitmap.height;
    if (w > maxDim || h > maxDim) {
        const scale = maxDim / Math.max(w, h);
        w = Math.round(w * scale);
        h = Math.round(h * scale);
    }
    const canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext('2d');
    ctx.drawImage(bitmap, 0, 0, w, h);
    bitmap.close();
    // 返回 JPEG data URL，通常只有 100-300KB
    return canvas.toDataURL('image/jpeg', quality);
}

/** 通过 Tauri 命令读取图片为 base64 data URL */
async function toAssetUrl(filePath) {
    if (!filePath) return '';
    try {
        const result = await window.sorterAPI.readImageDataUrl(filePath);
        return result || '';
    } catch (_) {
        return '';
    }
}

/**
 * 如果 data URL 对应的图片太大，用 canvas 压缩后再返回
 * 这样即使磁盘上是 1.81MB 的原图，传给 CSS 之前也会压缩到适合显示的大小 ✨
 */
async function compressDataUrl(dataUrl, maxDim = 1920, quality = 0.8) {
    try {
        const img = new Image();
        await new Promise((resolve, reject) => {
            img.onload = resolve;
            img.onerror = reject;
            img.src = dataUrl;
        });
        let w = img.width, h = img.height;
        if (w > maxDim || h > maxDim) {
            const scale = maxDim / Math.max(w, h);
            w = Math.round(w * scale);
            h = Math.round(h * scale);
        }
        const canvas = document.createElement('canvas');
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(img, 0, 0, w, h);
        return canvas.toDataURL('image/jpeg', quality);
    } catch {
        return dataUrl;
    }
}

/** 加载区域背景图（异步） */
async function loadZoneBg(i, filePath) {
    const dataUrl = await toAssetUrl(filePath);
    const dropZone = document.getElementById(`drop-${i}`);
    if (dropZone && dataUrl) {
        dropZone.style.backgroundImage = `url('${dataUrl}')`;
    }
}

// ─── 状态 ──────────────────────────────────────────
const state = {
    config:    { opacity: 0.95, bgOpacity: 0.40, globalBg: '', themeColor: '#e87890', chimeEnabled: true, minimizeToTray: false, closeToTray: true, zones: [] },
    isCutMode: false,
    isBusy:    false,
    timers:    {},       // 防抖计时器
};

const MAX_IMAGE_BYTES = 10 * 1024 * 1024;

// ─── DOM 引用 ──────────────────────────────────────
const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

const dom = {
    grid:            $('#grid'),
    toast:           $('#toast'),
    progressWrap:    $('#progress-wrap'),
    progressFill:    $('#progress-fill'),
    progressText:    $('#progress-text'),

    modeSwitch:      $('#mode-switch'),
    modeTrack:       $('#mode-track'),
    labelCopy:       $('#label-copy'),
    labelCut:        $('#label-cut'),

    btnSettings:     $('#btn-settings'),
    modalOverlay:    $('#modal-overlay'),
    modalClose:      $('#modal-close'),

    sliderOpacity:   $('#slider-opacity'),
    opacityValue:    $('#opacity-value'),
    sliderBgOpacity: $('#slider-bg-opacity'),
    bgOpacityValue:  $('#bg-opacity-value'),
    inputGlobalBg:   $('#input-global-bg'),
    zoneConfigList:  $('#zone-config-list'),
    dragPreview:     $('#drag-preview'),
    authorLink:      $('#author-link'),

    // 桃华定制：快照 DOM
    snapshotSelect:  $('#snapshot-select'),
    btnRestoreSnap:  $('#btn-restore-snapshot'),
    snapshotInfo:    $('#snapshot-info'),
};

// ─── 工具函数 ──────────────────────────────────────
function debounce(key, fn, ms = 350) {
    clearTimeout(state.timers[key]);
    state.timers[key] = setTimeout(fn, ms);
}

function formatBytes(bytes) {
    if (!bytes || bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    const val = (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0);
    return val + ' ' + units[i];
}

/** 将 hex 颜色变暗指定百分比 */
function darkenColor(hex, pct) {
    const num = parseInt(hex.slice(1), 16);
    const r = Math.max(0, Math.floor((num >> 16) * (1 - pct)));
    const g = Math.max(0, Math.floor(((num >> 8) & 0xff) * (1 - pct)));
    const b = Math.max(0, Math.floor((num & 0xff) * (1 - pct)));
    return '#' + (r << 16 | g << 8 | b).toString(16).padStart(6, '0');
}

function applyThemeColor(hex) {
    const root = document.documentElement.style;
    root.setProperty('--pink', hex);
    root.setProperty('--pink-hover', darkenColor(hex, 0.12));
    // 主题色浅色版（用于拖放区背景等）
    root.setProperty('--pink-light', hex + '14');
}

/** 播放完成提示音 */
function playChime() {
    try {
        const ctx = new (window.AudioContext || window.webkitAudioContext)();
        [880, 1100, 1320].forEach((freq, i) => {
            const osc = ctx.createOscillator();
            const gain = ctx.createGain();
            osc.type = 'sine';
            osc.frequency.value = freq;
            gain.gain.setValueAtTime(0.12, ctx.currentTime + i * 0.1);
            gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + i * 0.1 + 0.4);
            osc.connect(gain); gain.connect(ctx.destination);
            osc.start(ctx.currentTime + i * 0.1);
            osc.stop(ctx.currentTime + i * 0.1 + 0.5);
        });
    } catch (_) {}
}

function showToast(msg) {
    dom.toast.textContent = msg;
    dom.toast.style.opacity = '1';
    // 强制触发重绘（WebView2 需要）
    void dom.toast.offsetHeight;
    clearTimeout(state.timers.toast);
    state.timers.toast = setTimeout(() => {
        dom.toast.style.opacity = '0';
    }, 2000);
}

function validateImageFile(file) {
    if (!file) return false;
    if (!file.type.startsWith('image/')) {
        showToast('⚠️ 请选择图片文件');
        return false;
    }
    if (file.size > MAX_IMAGE_BYTES) {
        showToast('⚠️ 图片太大了，换一张 10MB 以内的吧');
        return false;
    }
    return true;
}

function toggleModal(show) {
    dom.modalOverlay.classList.toggle('show', show);
}

// ─── 进度事件监听 ──────────────────────────────────
const dragSpacer = document.querySelector('.drag-spacer');
const btnCancel = document.getElementById('btn-cancel');

btnCancel.addEventListener('click', () => {
    window.sorterAPI.cancelMove();
});

window.sorterAPI.onProgress((data) => {
    console.log('[progress]', data.status, data.percent ?? '', data.current ?? '', '/', data.total ?? '');
    if (data.status === 'start') {
        state.isBusy = true;
        if (dragSpacer) dragSpacer.style.display = 'none';
        btnCancel.style.display = '';
        dom.progressWrap.style.display = 'flex';
        dom.progressFill.style.width = '0%';
        dom.progressText.textContent = '正在处理文件……';
    } else if (data.status === 'progress') {
        dom.progressFill.style.width = data.percent + '%';
        const cur = formatBytes(data.current);
        const tot = formatBytes(data.total);
        dom.progressText.textContent = `${data.percent}%（${cur} / ${tot}）`;
    } else if (data.status === 'end') {
        state.isBusy = false;
        btnCancel.style.display = 'none';
        dom.progressFill.style.width = '100%';
        dom.progressText.textContent = '✨ 完成啦！';
        if (state.config.chimeEnabled) playChime();
        if (data.count) {
            const verb = state.isCutMode ? '移动' : '复制';
            showToast(`✅ 成功${verb}了 ${data.count} 个项目`);
        }
        setTimeout(() => {
            dom.progressWrap.style.display = 'none';
            if (dragSpacer) dragSpacer.style.display = '';
        }, 1800);
    } else if (data.status === 'cancelled') {
        state.isBusy = false;
        dom.progressWrap.style.display = 'none';
        if (dragSpacer) dragSpacer.style.display = '';
        showToast('🚫 操作已取消');
    } else if (data.status === 'error') {
        state.isBusy = false;
        dom.progressWrap.style.display = 'none';
        if (dragSpacer) dragSpacer.style.display = '';
        showToast(`❌ 出错了：${data.error}`);
    }
});

// ─── 窗口控制 ──────────────────────────────────────
document.getElementById('btn-minimize').addEventListener('click', () => {
    // 清除按钮 hover 状态，避免托盘恢复后粘住
    document.querySelectorAll('.win-btn.hovered').forEach(b => b.classList.remove('hovered'));
    window.sorterAPI.winMinimize();
});
document.getElementById('btn-close').addEventListener('click', () => {
    document.querySelectorAll('.win-btn.hovered').forEach(b => b.classList.remove('hovered'));
    window.sorterAPI.winClose();
});

// 窗口控制按钮的 hover 效果用 JS 事件代替 CSS :hover
// 解决 WebView2 从托盘恢复后 :hover 状态粘住的问题
document.querySelectorAll('.win-btn').forEach(btn => {
    btn.addEventListener('mouseenter', () => btn.classList.add('hovered'));
    btn.addEventListener('mouseleave', () => btn.classList.remove('hovered'));
});

// 托盘恢复窗口时，清除所有按钮的 hover 状态
listen('window-restored', () => {
    document.querySelectorAll('.win-btn.hovered').forEach(btn => {
        btn.classList.remove('hovered');
    });
});

// Alt+F4 / 任务栏关闭隐藏到托盘时，清除按钮 hover 状态
listen('window-hiding', () => {
    document.querySelectorAll('.win-btn.hovered').forEach(btn => {
        btn.classList.remove('hovered');
    });
});

// ─── JS 窗口拖拽（Tauri 版） ────────────────────────
{
    let dragging = false;
    let startX = 0, startY = 0;

    document.body.addEventListener('mousedown', (e) => {
        // 不拖拽按钮、输入框、拖放区、设置面板
        if (e.target.closest('button, input, .mode-switch, .win-controls, .header-right, .drop-zone, .card, .modal-overlay')) return;
        dragging = true;
        startX = e.screenX;
        startY = e.screenY;
    });

    window.addEventListener('mousemove', (e) => {
        if (!dragging) return;
        const dx = e.screenX - startX;
        const dy = e.screenY - startY;
        if (Math.abs(dx) > 1 || Math.abs(dy) > 1) {
            window.sorterAPI.winMove({ dx, dy });
            startX = e.screenX;
            startY = e.screenY;
        }
    });

    window.addEventListener('mouseup', () => {
        dragging = false;
    });
}

// ─── 模式切换 ──────────────────────────────────────
dom.modeSwitch.addEventListener('click', () => {
    state.isCutMode = !state.isCutMode;

    if (state.isCutMode) {
        dom.modeTrack.className = 'track cut';
        dom.labelCut.classList.add('cut-active');
        dom.labelCopy.classList.remove('copy-active');
        showToast('✂️ 已切换到剪切模式');
    } else {
        dom.modeTrack.className = 'track copy';
        dom.labelCopy.classList.add('copy-active');
        dom.labelCut.classList.remove('cut-active');
        showToast('📋 已切换到复制模式');
    }
});

// ─── 渲染所有区域 ──────────────────────────────────
function renderAllZones() {
    // 清空
    dom.grid.innerHTML = '';
    dom.zoneConfigList.innerHTML = '';

    state.config.zones.forEach((zone, i) => {
        renderZoneCard(zone, i);
        renderZoneConfigCard(zone, i);
    });
}

function renderZoneCard(zone, i) {
    const card = document.createElement('div');
    card.className = 'card';

    card.innerHTML = `
        <div class="drop-zone ${zone.bg ? 'has-bg' : ''}"
             id="drop-${i}"
             style="">
            <div class="title" id="title-${i}">${escapeHTML(zone.name)}</div>
        </div>
        <div class="input-row">
            <input type="text"
                   id="path-input-${i}"
                   value="${escapeHTML(zone.path)}"
                   placeholder="拖入文件后保存到……">
            <button class="save-btn saved" id="save-btn-${i}">✓</button>
        </div>
    `;

    dom.grid.appendChild(card);
    bindZoneEvents(i);
    // 异步加载区域背景图（base64 data URL）
    if (zone.bg) loadZoneBg(i, zone.bg);
}

function renderZoneConfigCard(zone, i) {
    const card = document.createElement('div');
    card.className = 'zone-config-card';

    card.innerHTML = `
        <div class="index-badge" title="区域 ${i + 1}">${i + 1}</div>
        <input type="text"
               id="zone-name-${i}"
               class="zone-name-input"
               value="${escapeHTML(zone.name)}"
               placeholder="名称"
               title="修改区域名称">
        <div class="zone-btns">
            <label class="zone-btn" title="更换区域背景图片">
                🖼️
                <input type="file" id="zone-bg-${i}" accept="image/*" hidden>
            </label>
            <button class="zone-btn zone-btn--del" id="zone-bg-reset-${i}" title="移除区域背景图片">✕</button>
        </div>
    `;

    dom.zoneConfigList.appendChild(card);
    bindZoneConfigEvents(i);
}

function escapeHTML(str) {
    const escapes = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
    return String(str).replace(/[&<>"']/g, ch => escapes[ch]);
}

// ─── 区域事件绑定 ──────────────────────────────────

function bindZoneEvents(i) {
    const zone      = state.config.zones[i];
    const dropZone  = document.getElementById(`drop-${i}`);
    const pathInput = document.getElementById(`path-input-${i}`);
    const saveBtn   = document.getElementById(`save-btn-${i}`);
    const titleEl   = document.getElementById(`title-${i}`);

    // 双击打开文件夹
    dropZone.addEventListener('dblclick', () => {
        if (zone.path) {
            window.sorterAPI.openFolder(zone.path).catch(err => {
                showToast(`❌ ${err}`);
            });
        }
    });

    // 路径输入 → 显示保存按钮
    pathInput.addEventListener('input', () => {
        const modified = pathInput.value.trim() !== zone.path;
        saveBtn.className = modified ? 'save-btn modified' : 'save-btn saved';
    });

    // 保存路径
    saveBtn.addEventListener('click', async () => {
        if (!saveBtn.classList.contains('modified')) return;
        zone.path = pathInput.value.trim();
        await persistConfig(true);  // 路径变更 → 创建快照
        saveBtn.className = 'save-btn saved';
        showToast('✅ 路径已保存');
    });

    const card = dropZone.closest('.card');

    // 拖放事件 —— 仅做 preventDefault + 视觉反馈
    // 实际文件路径由 Tauri 原生 on_drag_drop_event 提供（WebView2 没有 file.path）
    ['dragenter', 'dragover', 'dragleave', 'drop'].forEach(evt => {
        dropZone.addEventListener(evt, e => { e.preventDefault(); e.stopPropagation(); });
    });

    dropZone.addEventListener('dragover', () => {
        dropZone.classList.add('drag-over');
        card.classList.add('drag-active');
    });
    dropZone.addEventListener('dragleave', () => {
        dropZone.classList.remove('drag-over');
        card.classList.remove('drag-active');
    });
    dropZone.addEventListener('drop', () => {
        dropZone.classList.remove('drag-over');
        card.classList.remove('drag-active');
    });
}

function bindZoneConfigEvents(i) {
    const zone       = state.config.zones[i];
    const nameInput  = document.getElementById(`zone-name-${i}`);
    const bgInput    = document.getElementById(`zone-bg-${i}`);
    const resetBtn   = document.getElementById(`zone-bg-reset-${i}`);
    const titleEl    = document.getElementById(`title-${i}`);
    const dropZone   = document.getElementById(`drop-${i}`);
    resetBtn.disabled = !zone.bg;

    // 名称变更
    nameInput.addEventListener('input', () => {
        zone.name = nameInput.value.trim() || `区域 ${i + 1}`;
        if (titleEl) titleEl.textContent = zone.name;
        debounce(`zone-name-${i}`, () => persistConfig(true), 500);  // 名称变更 → 创建快照
    });

    // 更换背景
    bgInput.addEventListener('change', async (e) => {
        const file = e.target.files[0];
        if (!file) return;
        if (!validateImageFile(file)) {
            bgInput.value = '';
            return;
        }

        try {
            // 压缩图片以避免 WebView2 IPC 大消息挂起
            const dataUrl = await compressImageForIpc(file);
            const base64 = dataUrl.split(',')[1];
            const saved = await window.sorterAPI.importImage(file.name, base64);
            if (!saved) return;
            zone.bg = saved;
            if (dropZone) {
                dropZone.style.backgroundImage = `url('${dataUrl}')`;
                dropZone.classList.add('has-bg');
            }
            resetBtn.disabled = false;
            await persistConfig(true);  // 区域背景变更 → 创建快照
            showToast('🖼️ 区域背景已更新');
        } catch (err) {
            showToast(`❌ ${err}`);
        } finally {
            bgInput.value = '';
        }
    });

    // 移除背景
    resetBtn.addEventListener('click', async () => {
        zone.bg = '';
        if (dropZone) {
            dropZone.style.backgroundImage = '';
            dropZone.classList.remove('has-bg');
        }
        resetBtn.disabled = true;
        await persistConfig(true);  // 移除区域背景 → 创建快照
        showToast('已移除区域背景');
    });
}

// ─── 背景亮度检测 & 标题颜色自适应 ─────────────────

function detectTitleBrightness() {
    return new Promise((resolve) => {
        const bgImage = getComputedStyle(document.documentElement).getPropertyValue('--bg-image').trim();
        // 没有背景图 → 默认浅色背景 → 深色文字
        if (!bgImage || bgImage === 'none') {
            resolve('light-bg');
            return;
        }

        const url = bgImage.replace(/url\(['"]?/g, '').replace(/['"]?\)/g, '');
        const img = new Image();

        img.onload = () => {
            const canvas = document.createElement('canvas');
            const size = 40;
            canvas.width = size;
            canvas.height = size;
            const ctx = canvas.getContext('2d');
            // 采样左上角（标题所在区域）
            ctx.drawImage(img, 0, 0, size, size, 0, 0, size, size);

            const data = ctx.getImageData(0, 0, size, size).data;
            let totalLum = 0, count = 0;
            for (let i = 0; i < data.length; i += 16) {
                const r = data[i], g = data[i + 1], b = data[i + 2];
                // 感知亮度公式
                totalLum += 0.299 * r + 0.587 * g + 0.114 * b;
                count++;
            }
            const avgLum = totalLum / count;
            resolve(avgLum > 128 ? 'light-bg' : 'dark-bg');
        };

        img.onerror = () => resolve('light-bg');
        img.src = url;
    });
}

async function updateTitleContrast() {
    const result = await detectTitleBrightness();
    const title = document.querySelector('.app-title');
    const btns  = document.querySelectorAll('.win-btn');

    if (result === 'dark-bg') {
        // 深色背景 → 白色文字 + 暗影
        title.style.color = '#ffffff';
        title.style.textShadow = '0 1px 4px rgba(0,0,0,0.5)';
        btns.forEach(b => {
            b.style.color = '#ddd';
            b.style.borderColor = 'rgba(255,255,255,0.25)';
        });
    } else {
        // 浅色背景 → 深色文字 + 亮影
        title.style.color = '#5a4f53';
        title.style.textShadow = '0 1px 2px rgba(255,255,255,0.4)';
        btns.forEach(b => {
            b.style.color = '';
            b.style.borderColor = '';
        });
    }
}

// ─── 全局设置事件 ──────────────────────────────────

// 透明度滑块
dom.sliderOpacity.addEventListener('input', (e) => {
    const val = parseFloat(e.target.value);
    state.config.opacity = val;
    document.documentElement.style.setProperty('--glass-opacity', val);
    window.sorterAPI.setOpacity(val);
    dom.opacityValue.textContent = Math.round(val * 100) + '%';
    debounce('opacity', persistConfig, 300);
});

// 背景透明度滑块
dom.sliderBgOpacity.addEventListener('input', (e) => {
    const val = parseFloat(e.target.value);
    state.config.bgOpacity = val;
    document.documentElement.style.setProperty('--bg-opacity', val);
    dom.bgOpacityValue.textContent = Math.round(val * 100) + '%';
    debounce('bgOpacity', persistConfig, 300);
});

// 背景图选择
const btnPickBg = document.getElementById('btn-pick-bg');
const btnClearBg = document.getElementById('btn-clear-bg');
const bgStatus = document.getElementById('bg-status');

btnPickBg.addEventListener('click', () => dom.inputGlobalBg.click());

dom.inputGlobalBg.addEventListener('change', async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    if (!validateImageFile(file)) {
        dom.inputGlobalBg.value = '';
        return;
    }

    try {
        // 压缩图片以避免 WebView2 IPC 大消息挂起（>1MB 的 base64 会卡死）
        const dataUrl = await compressImageForIpc(file);
        const base64 = dataUrl.split(',')[1];
        const saved = await window.sorterAPI.importImage(file.name, base64);
        if (!saved) return;
        state.config.globalBg = saved;
        document.documentElement.style.setProperty('--bg-image', `url('${dataUrl}')`);
        updateTitleContrast();
        updateBgStatus();
        await persistConfig();
        showToast('🖼️ 主背景已更新');
    } catch (err) {
        showToast(`❌ ${err}`);
    } finally {
        dom.inputGlobalBg.value = '';
    }
});

btnClearBg.addEventListener('click', async () => {
    state.config.globalBg = '';
    document.documentElement.style.setProperty('--bg-image', 'none');
    updateTitleContrast();
    updateBgStatus();
    await persistConfig();
    showToast('已清除背景图');
});

function updateBgStatus() {
    bgStatus.textContent = state.config.globalBg ? '已设置背景图' : '未设置背景图';
    btnClearBg.disabled = !state.config.globalBg;
}

// 作者链接
dom.authorLink.addEventListener('click', () => {
    window.sorterAPI.openExternal('https://space.bilibili.com/14816');
});

// GitHub 图标
document.querySelector('.github-icon').addEventListener('click', (e) => {
    e.preventDefault();
    window.sorterAPI.openExternal('https://github.com/TohnoSeika');
});

// ─── 主题颜色 ──────────────────────────────────────
const inputThemeColor = document.getElementById('input-theme-color');
const btnResetTheme = document.getElementById('btn-reset-theme');

inputThemeColor.addEventListener('input', (e) => {
    const hex = e.target.value;
    state.config.themeColor = hex;
    applyThemeColor(hex);
    debounce('themeColor', persistConfig, 300);
});

btnResetTheme.addEventListener('click', () => {
    state.config.themeColor = '#e87890';
    applyThemeColor('#e87890');
    inputThemeColor.value = '#e87890';
    persistConfig();
});

// ─── 托盘开关 ──────────────────────────────────────
const toggleMinimizeTray = document.getElementById('toggle-minimize-tray');
const toggleCloseTray = document.getElementById('toggle-close-tray');
const toggleChime = document.getElementById('toggle-chime');

toggleMinimizeTray.addEventListener('click', () => {
    state.config.minimizeToTray = !state.config.minimizeToTray;
    toggleMinimizeTray.classList.toggle('toggle--on', state.config.minimizeToTray);
    persistConfig();
});

toggleCloseTray.addEventListener('click', () => {
    state.config.closeToTray = !state.config.closeToTray;
    toggleCloseTray.classList.toggle('toggle--on', state.config.closeToTray);
    persistConfig();
});

toggleChime.addEventListener('click', () => {
    state.config.chimeEnabled = !state.config.chimeEnabled;
    toggleChime.classList.toggle('toggle--on', state.config.chimeEnabled);
    persistConfig();
});

// ─── 模态框 ────────────────────────────────────────
dom.btnSettings.addEventListener('click', () => {
    toggleModal(true);
    refreshSnapshotList();
});
dom.modalClose.addEventListener('click', () => toggleModal(false));

// 记录 mousedown 是否在模态框内，避免拖选文字松手在外时误关
let clickStartedInside = false;
dom.modalOverlay.addEventListener('mousedown', (e) => {
    clickStartedInside = e.target !== dom.modalOverlay;
});
dom.modalOverlay.addEventListener('click', (e) => {
    if (e.target === dom.modalOverlay && !clickStartedInside) toggleModal(false);
});

// ─── 操作历史 —— 由 history.js 独立实现 ──────────

// ─── 操作说明弹出框 ──────────────────────────────
const domGuide = {
    overlay: $('#guide-overlay'),
    close:   $('#guide-close'),
};
$('#btn-guide').addEventListener('click', () => {
    domGuide.overlay.classList.add('show');
});
domGuide.close.addEventListener('click', () => {
    domGuide.overlay.classList.remove('show');
});
domGuide.overlay.addEventListener('click', (e) => {
    if (e.target === domGuide.overlay) domGuide.overlay.classList.remove('show');
});

// ESC 关闭所有模态框
document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        toggleModal(false);
        domGuide.overlay.classList.remove('show');
    }
});

// 桃华定制：快照管理 ────────────────────────────────

let snapshotList = [];

function formatSnapshotLabel(ts) {
    // Unix 秒 → 本地时间字符串，如 "2026年6月18日 19:02:07"
    const d = new Date(ts * 1000);
    return d.getFullYear() + '年' + (d.getMonth() + 1) + '月' + d.getDate() + '日 '
        + String(d.getHours()).padStart(2, '0') + ':'
        + String(d.getMinutes()).padStart(2, '0') + ':'
        + String(d.getSeconds()).padStart(2, '0');
}

async function refreshSnapshotList() {
    try {
        snapshotList = await window.sorterAPI.listSnapshots();
    } catch {
        snapshotList = [];
    }

    const sel = dom.snapshotSelect;
    sel.innerHTML = '<option value="">— 选择快照（共 ' + snapshotList.length + ' 份）—</option>';
    snapshotList.forEach((snap, i) => {
        const opt = document.createElement('option');
        opt.value = snap.name;
        opt.textContent = formatSnapshotLabel(snap.timestamp) + (i === 0 ? ' （最新）' : '');
        sel.appendChild(opt);
    });

    sel.value = '';
    dom.btnRestoreSnap.disabled = true;
    dom.snapshotInfo.textContent = '';
}

dom.snapshotSelect.addEventListener('change', () => {
    const name = dom.snapshotSelect.value;
    if (!name) {
        dom.btnRestoreSnap.disabled = true;
        dom.snapshotInfo.textContent = '';
        return;
    }
    const snap = snapshotList.find(s => s.name === name);
    if (snap) {
        dom.btnRestoreSnap.disabled = false;
        const sizeKB = (snap.configSize / 1024).toFixed(1);
        dom.snapshotInfo.textContent =
            formatSnapshotLabel(snap.timestamp)
            + ' · ' + snap.imageCount + ' 张背景图 · ' + sizeKB + ' KB';
    }
});

async function applyCurrentConfig() {
    // 重新应用全部 UI 设置（从 state.config）
    document.documentElement.style.setProperty('--glass-opacity', state.config.opacity);
    window.sorterAPI.setOpacity(state.config.opacity);
    dom.sliderOpacity.value = state.config.opacity;
    dom.opacityValue.textContent = Math.round(state.config.opacity * 100) + '%';

    document.documentElement.style.setProperty('--bg-opacity', state.config.bgOpacity ?? 1);
    dom.sliderBgOpacity.value = state.config.bgOpacity ?? 1;
    dom.bgOpacityValue.textContent = Math.round((state.config.bgOpacity ?? 1) * 100) + '%';

    if (state.config.globalBg) {
        const dataUrl = await toAssetUrl(state.config.globalBg);
        document.documentElement.style.setProperty('--bg-image', dataUrl ? `url('${dataUrl}')` : 'none');
    } else {
        document.documentElement.style.setProperty('--bg-image', 'none');
    }

    inputThemeColor.value = state.config.themeColor || '#e87890';
    applyThemeColor(state.config.themeColor || '#e87890');

    toggleMinimizeTray.classList.toggle('toggle--on', state.config.minimizeToTray);
    toggleCloseTray.classList.toggle('toggle--on', state.config.closeToTray);
    toggleChime.classList.toggle('toggle--on', state.config.chimeEnabled);

    renderAllZones();
    updateTitleContrast();
    updateBgStatus();
}

dom.btnRestoreSnap.addEventListener('click', async () => {
    const name = dom.snapshotSelect.value;
    if (!name) return;

    try {
        await window.sorterAPI.restoreSnapshot(name);
        state.config = await window.sorterAPI.loadConfig();
        await applyCurrentConfig();
        showToast('✅ 已恢复到 ' + formatSnapshotLabel(
            snapshotList.find(s => s.name === name)?.timestamp || 0
        ));
    } catch (err) {
        showToast('❌ 恢复失败：' + err);
    }
});

// ─── 配置持久化 ────────────────────────────────────
// 桃华定制：createSnapshot=true 仅用于改区域名/路径/背景图
async function persistConfig(createSnapshot = false) {
    await window.sorterAPI.saveConfig(state.config, createSnapshot);
}

// ─── 初始化 ────────────────────────────────────────
async function init() {
    state.config = await window.sorterAPI.loadConfig();

    // 应用全局设置
    document.documentElement.style.setProperty('--glass-opacity', state.config.opacity);
    window.sorterAPI.setOpacity(state.config.opacity);
    dom.sliderOpacity.value = state.config.opacity;
    dom.opacityValue.textContent = Math.round(state.config.opacity * 100) + '%';

    document.documentElement.style.setProperty('--bg-opacity', state.config.bgOpacity ?? 1);
    dom.sliderBgOpacity.value = state.config.bgOpacity ?? 1;
    dom.bgOpacityValue.textContent = Math.round((state.config.bgOpacity ?? 1) * 100) + '%';

    if (state.config.globalBg) {
        const dataUrl = await toAssetUrl(state.config.globalBg);
        if (dataUrl) {
            const compressed = dataUrl.length > 500 * 1024 ? await compressDataUrl(dataUrl) : dataUrl;
            document.documentElement.style.setProperty('--bg-image', `url('${compressed}')`);
        }
    }

    renderAllZones();
    updateTitleContrast();
    updateBgStatus();

    toggleMinimizeTray.classList.toggle('toggle--on', state.config.minimizeToTray);
    toggleCloseTray.classList.toggle('toggle--on', state.config.closeToTray);
    toggleChime.classList.toggle('toggle--on', state.config.chimeEnabled);

    inputThemeColor.value = state.config.themeColor || '#e87890';
    applyThemeColor(state.config.themeColor || '#e87890');
}

// ─── Tauri 原生拖放事件 ─────────────────────────
// WebView2 的 DataTransfer 没有 file.path（那是 Electron 专有 API）
// 这里监听 Rust 端发射的原生拖放事件，拿到真实文件路径
{
    const { listen } = window.__TAURI__.event;

    let dragActiveZone = null;

    listen('tauri-drag-enter', (event) => {
        const { count, x, y } = event.payload;
        const el = document.elementFromPoint(x, y);
        const zone = el?.closest('.drop-zone');
        if (zone) {
            zone.classList.add('drag-over');
            zone.closest('.card')?.classList.add('drag-active');
            dragActiveZone = zone;
        }
        if (count) {
            const verb = state.isCutMode ? '剪切' : '复制';
            dom.dragPreview.textContent = `${verb} ${count} 个项目`;
            dom.dragPreview.classList.add('show');
        }
    });

    listen('tauri-drag-over', (event) => {
        const { x, y } = event.payload;
        const el = document.elementFromPoint(x, y);
        const zone = el?.closest('.drop-zone');
        if (zone !== dragActiveZone) {
            // 离开旧区域
            if (dragActiveZone) {
                dragActiveZone.classList.remove('drag-over');
                dragActiveZone.closest('.card')?.classList.remove('drag-active');
            }
            // 进入新区域
            if (zone) {
                zone.classList.add('drag-over');
                zone.closest('.card')?.classList.add('drag-active');
            }
            dragActiveZone = zone;
        }
        dom.dragPreview.style.left = (x + 16) + 'px';
        dom.dragPreview.style.top  = (y - 30) + 'px';
    });

    listen('tauri-drag-leave', () => {
        if (dragActiveZone) {
            dragActiveZone.classList.remove('drag-over');
            dragActiveZone.closest('.card')?.classList.remove('drag-active');
            dragActiveZone = null;
        }
        dom.dragPreview.classList.remove('show');
    });

    listen('tauri-drop', (event) => {
        const { paths, x, y } = event.payload;
        if (dragActiveZone) {
            dragActiveZone.classList.remove('drag-over');
            dragActiveZone.closest('.card')?.classList.remove('drag-active');
            dragActiveZone = null;
        }
        dom.dragPreview.classList.remove('show');

        if (!paths || !paths.length) return;

        const el = document.elementFromPoint(x, y);
        const dropZone = el?.closest('.drop-zone');
        if (!dropZone) return;

        const zoneIndex = parseInt(dropZone.id.split('-')[1]);
        const zone = state.config.zones[zoneIndex];
        if (state.isBusy) {
            showToast('还在处理上一批文件，等一下……');
            return;
        }
        if (!zone || !zone.path) {
            showToast('⚠️ 请先设置目标路径哦');
            return;
        }

        const mode = state.isCutMode ? 'cut' : 'copy';
        state.isBusy = true;
        window.sorterAPI.moveFiles({ filePaths: paths, targetDir: zone.path, mode }).catch((err) => {
            state.isBusy = false;
            showToast(`❌ ${err}`);
        });
    });
}

init().catch(err => {
    console.error('初始化失败:', err);
    showToast('😢 初始化出错了，试试重启？');
});
