/**
 * Minagi 文件分类助手 — 操作历史
 * 桃华整理：直接使用 index.html 里的历史面板，不再自己造一个一模一样的
 * 这样主题色、滚动条、弹窗动画都能和设置面板保持一致
 */
(function () {
    const overlay  = document.getElementById('history-overlay');
    const listEl   = document.getElementById('history-list');
    const emptyEl  = document.getElementById('history-empty');
    const btnOpen  = document.getElementById('btn-history');
    const btnClose = document.getElementById('history-close');
    const btnClear = document.getElementById('btn-clear-history');
    if (!overlay || !listEl || !btnOpen) return;

    function esc(s) {
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    function trunc(name, max) {
        if (name.length <= max) return name;
        const dot = name.lastIndexOf('.');
        const ext = dot > 0 ? name.substring(dot) : '';
        const keep = max - ext.length - 1;
        if (keep < 3) return name.substring(0, max - 3) + '…';
        return name.substring(0, keep) + '…' + ext;
    }

    function parentDir(path) {
        const sep = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
        return sep > 0 ? path.substring(0, sep) : path;
    }

    const pad = (n) => String(n).padStart(2, '0');

    /** 时间列：短一点，完整时间放在 tooltip 里 */
    function shortTime(timestamp) {
        if (!timestamp) return '—';
        const d = new Date(timestamp * 1000);
        return `${d.getMonth() + 1}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    }

    function fullTime(timestamp) {
        if (!timestamp) return '';
        const d = new Date(timestamp * 1000);
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} `
            + `${pad(d.getHours())}:${pad(d.getMinutes())}`;
    }

    function folderIcon(path) {
        return '<span class="history-folder-icon" data-path="' + esc(path) + '" title="' + esc(path) + '">'
            + '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">'
            + '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2v11z"/>'
            + '</svg></span>';
    }

    function describeItems(entry) {
        const names = entry.file_names || [];
        const count = entry.item_count || names.length;

        if (count === 1) {
            const fileName = names[0] || '';
            return { text: trunc(fileName, 24), tooltip: fileName };
        }

        const tooltip = names.length <= 3
            ? names.join('\n')
            : names.slice(0, 3).join('\n') + '\n… 等 ' + names.length + ' 个文件';
        return { text: count + ' 个项目', tooltip };
    }

    function renderEntry(entry) {
        const row = document.createElement('div');
        row.className = 'history-row';

        const mode = entry.mode === 'cut'
            ? { icon: '✂️', cls: 'cut' }
            : { icon: '📋', cls: 'copy' };
        const items = describeItems(entry);

        row.innerHTML =
            '<span class="history-col history-col--src">' +
            folderIcon(parentDir((entry.source_paths || [])[0] || '')) +
            '</span>' +
            '<span class="history-col history-col--name">' +
            '<span class="history-mode ' + mode.cls + '">' + mode.icon + '</span>' +
            '<span class="history-name" title="' + esc(items.tooltip) + '">' + esc(items.text) + '</span>' +
            '</span>' +
            '<span class="history-col history-col--dst">' +
            folderIcon(entry.target_dir || '') +
            '</span>' +
            '<span class="history-col history-col--time" title="' + esc(fullTime(entry.timestamp)) + '">' +
            esc(shortTime(entry.timestamp)) +
            '</span>';

        return row;
    }

    async function loadHistory() {
        let history = [];
        try {
            history = (await window.sorterAPI.loadHistory()) || [];
        } catch (_) {
            history = [];
        }

        listEl.innerHTML = '';
        emptyEl.classList.toggle('hidden', history.length > 0);

        // 最新的排在最上面
        history.slice().reverse().forEach((entry) => {
            listEl.appendChild(renderEntry(entry));
        });
    }

    // 文件夹图标用事件委托，列表重建后也不用重新绑定
    listEl.addEventListener('click', (e) => {
        const target = e.target.closest('[data-path]');
        if (!target) return;
        window.sorterAPI.openFolder(target.dataset.path).catch(() => {});
    });

    // 「清空」要点两次：第一次只是亮红灯，三秒内不确认就恢复
    let clearArmed = false;
    let clearTimer = null;

    function resetClearButton() {
        clearArmed = false;
        clearTimeout(clearTimer);
        btnClear.textContent = '清空';
        btnClear.classList.remove('set-btn--danger');
    }

    btnClear.addEventListener('click', async () => {
        if (!clearArmed) {
            clearArmed = true;
            btnClear.textContent = '确定清空？';
            btnClear.classList.add('set-btn--danger');
            clearTimer = setTimeout(resetClearButton, 3000);
            return;
        }

        try {
            await window.sorterAPI.clearHistory();
            await loadHistory();
            resetClearButton();
            if (typeof showToast === 'function') showToast('🧹 操作历史已清空');
        } catch (err) {
            if (typeof showToast === 'function') showToast(`❌ 清空失败：${err}`, 4000);
        }
    });

    function open() {
        overlay.classList.add('show');
        loadHistory();
    }

    function close() {
        overlay.classList.remove('show');
        resetClearButton();
    }

    btnOpen.addEventListener('click', open);
    btnClose.addEventListener('click', close);
    overlay.addEventListener('click', (e) => {
        if (e.target === overlay) close();
    });
    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && overlay.classList.contains('show')) close();
    });
})();
