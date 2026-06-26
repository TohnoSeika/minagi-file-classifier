/**
 * Minagi 文件分类助手 — 操作历史（100% 独立版）
 * 所有 DOM / CSS / 事件完全自建，不与任何已有代码共享
 */
(function () {
    var btn = document.getElementById('btn-history');
    if (!btn) return;

    function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }
    function trunc(name, max) {
        if (name.length <= max) return name;
        var dot = name.lastIndexOf('.');
        var ext = dot > 0 ? name.substring(dot) : '';
        var keep = max - ext.length - 1;
        if (keep < 3) return name.substring(0, max - 3) + '…';
        return name.substring(0, keep) + '…' + ext;
    }

    btn.addEventListener('click', function () {
        // 100% 动态构建，不碰任何已有 DOM
        var mask = document.createElement('div');
        mask.style.cssText = 'position:fixed;inset:0;z-index:9999;background:rgba(40,30,28,0.25);backdrop-filter:blur(6px);display:flex;justify-content:center;align-items:center;';

        var modal = document.createElement('div');
        modal.style.cssText = 'width:500px;max-height:84vh;background:rgba(255,255,255,0.92);backdrop-filter:blur(12px);border-radius:16px;padding:20px 22px;box-shadow:0 8px 40px rgba(60,40,35,0.15);display:flex;flex-direction:column;gap:14px;overflow:hidden;';

        // 标题栏
        var titleBar = document.createElement('div');
        titleBar.style.cssText = 'display:flex;justify-content:space-between;align-items:center;';
        var titleH2 = document.createElement('h2');
        titleH2.textContent = '🕐 操作历史';
        titleH2.style.cssText = 'font-size:17px;color:#4a4044;margin:0;';
        var closeBtn = document.createElement('span');
        closeBtn.textContent = '×';
        closeBtn.style.cssText = 'font-size:22px;cursor:pointer;color:#a09896;';
        titleBar.appendChild(titleH2);
        titleBar.appendChild(closeBtn);

        // 表头行
        var headRow = document.createElement('div');
        headRow.style.cssText = 'display:flex;gap:12px;padding:10px 14px;border-bottom:1px solid rgba(0,0,0,0.06);font-size:12px;font-weight:500;color:#a09896;text-transform:uppercase;letter-spacing:0.04em;background:#ffffff;flex-shrink:0;';
        headRow.innerHTML = '<span style="flex:0 0 52px;display:flex;justify-content:center;">原始路径</span><span style="flex:1;min-width:0;text-align:center;">文件详情</span><span style="flex:0 0 52px;display:flex;justify-content:center;">目标路径</span>';

        // 滚动列表区
        var scrollArea = document.createElement('div');
        scrollArea.className = 'settings-scroll';
        scrollArea.style.cssText = 'overflow:hidden auto;flex:1;max-height:calc(84vh - 120px);border-radius:12px;background:rgba(255,255,255,0.6);padding:0;';

        // 列表容器
        var listEl = document.createElement('div');
        var emptyEl = document.createElement('div');
        emptyEl.textContent = '暂无操作记录 ✨';
        emptyEl.style.cssText = 'text-align:center;padding:20px;font-size:13.5px;color:#a09896;';
        scrollArea.appendChild(listEl);
        scrollArea.appendChild(emptyEl);

        modal.appendChild(titleBar);
        modal.appendChild(headRow);
        modal.appendChild(scrollArea);
        mask.appendChild(modal);
        document.body.appendChild(mask);

        // 加载数据
        if (window.sorterAPI) {
            window.sorterAPI.loadHistory().then(function (h) {
                listEl.innerHTML = '';
                if (!h || !h.length) { emptyEl.style.display = ''; return; }
                emptyEl.style.display = 'none';
                h.reverse().forEach(function (e) {
                    var r = document.createElement('div');
                    r.style.cssText = 'display:flex;gap:12px;padding:10px 14px;border-bottom:1px solid rgba(0,0,0,0.03);align-items:center;font-size:13px;';
                    var sp = e.source_paths[0] || '';
                    var sep = Math.max(sp.lastIndexOf('\\'), sp.lastIndexOf('/'));
                    var sd = sep > 0 ? sp.substring(0, sep) : sp;
                    var ml = e.mode === 'cut' ? '✂️' : '📋';
                    var mc = e.mode === 'cut' ? 'cut' : 'copy';
                    var nm = '', nt = '';
                    if (e.item_count === 1) { var fn = e.file_names[0] || ''; nm = trunc(fn, 24); nt = fn; }
                    else {
                        nm = e.item_count + ' 个项目';
                        // tooltip 显示前 3 个文件名，超出则加省略提示
                        var _fns = e.file_names;
                        if (_fns.length <= 3) { nt = _fns.join('\n'); }
                        else { nt = _fns.slice(0, 3).join('\n') + '\n… 等 ' + _fns.length + ' 个文件'; }
                    }
                    r.innerHTML =
                        '<span style="flex:0 0 52px;display:flex;justify-content:center;">' +
                        '<span style="cursor:pointer;width:28px;height:28px;display:flex;align-items:center;justify-content:center;color:#e87890;border-radius:8px;" data-path="'+esc(sd)+'" title="'+esc(sd)+'">' +
                        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" style="width:20px;height:20px;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2v11z"/></svg></span></span>' +
                        '<span style="flex:1;min-width:0;text-align:center;display:flex;align-items:center;justify-content:center;gap:4px;overflow:hidden;white-space:nowrap;">' +
                        '<span style="display:inline-block;flex:0 0 auto;font-size:11px;padding:1px 6px;border-radius:99px;background:rgba(0,0,0,0.05);'+(e.mode==='cut'?'color:#e87890;':'color:#5b9e8a;')+'">'+ml+'</span>' +
                        '<span style="min-width:0;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#5a4f53;font-weight:500;font-size:13px;"'+(nt?' title="'+esc(nt)+'"':'')+'>'+esc(nm)+'</span></span>' +
                        '<span style="flex:0 0 52px;display:flex;justify-content:center;">' +
                        '<span style="cursor:pointer;width:28px;height:28px;display:flex;align-items:center;justify-content:center;color:#e87890;border-radius:8px;" data-path="'+esc(e.target_dir)+'" title="'+esc(e.target_dir)+'">' +
                        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" style="width:20px;height:20px;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2v11z"/></svg></span></span>';
                    r.querySelectorAll('[data-path]').forEach(function(el){
                        el.addEventListener('click',function(){window.sorterAPI.openFolder(el.getAttribute('data-path')).catch(function(){});});
                    });
                    listEl.appendChild(r);
                });
            }).catch(function(){});
        }

        // 关闭
        function close() { mask.remove(); }
        closeBtn.addEventListener('click', close);
        mask.addEventListener('click', function(e){ if(e.target===mask) close(); });
        document.addEventListener('keydown', function escHandler(e){ if(e.key==='Escape'){close();document.removeEventListener('keydown',escHandler);} });
    });
})();
