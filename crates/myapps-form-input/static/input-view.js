// Saved-input table: inline cell editing, sort, search, and add/delete/move row.
//
// Labels and endpoint URLs arrive as one JSON blob in #view-config, so nothing
// is interpolated into this file.

(function() {
    var cfg = JSON.parse(document.getElementById('view-config').textContent);
    var lblBool = cfg.labels.bool;
    var lblLinkDefault = cfg.labels.linkDefault;
    var lblDeleteRow = cfg.labels.deleteRow;
    var lblRowOpFailed = cfg.labels.rowOpFailed;
    var saveUrl = cfg.urls.save;
    var addRowUrl = cfg.urls.addRow;
    var delRowUrl = cfg.urls.delRow;
    var moveRowUrl = cfg.urls.moveRow;
    var lblMoveRow = cfg.labels.moveRow;
    var lblMoveRowOff = cfg.labels.moveRowOff;

    // ── Sort & search ──────────────────────────────────────────
    // Operates purely on the DOM. Cells keep their original
    // data-row/data-col, so saves still hit the underlying CSV row
    // regardless of the visible order or which rows are filtered out.
    var tbody = document.querySelector('.ci-input-table tbody');
    var mainRows = tbody ? Array.from(tbody.querySelectorAll('tr.ci-main-row')) : [];
    // Pair each main row with its (optional) multiline follow-up so
    // sort moves them together and search hides them together.
    var rowGroups = mainRows.map(function(tr) {
        var follow = tbody.querySelector('tr.ci-multiline-row[data-row="' + tr.dataset.row + '"]');
        return { main: tr, follow: follow };
    });
    var activeSort = null;      // { col: idx, dir: 'asc'|'desc', type: '...' }
    var activeSearch = '';

    function cellTextAt(tr, colIdx) {
        var c = tr.querySelector('[data-col="' + colIdx + '"]');
        return c ? (c.textContent || '').trim() : '';
    }

    function rowHaystack(g) {
        var s = g.main.textContent;
        if (g.follow) s += ' ' + g.follow.textContent;
        return s.toLowerCase();
    }

    function applySearch() {
        rowGroups.forEach(function(g) {
            var visible = !activeSearch || rowHaystack(g).indexOf(activeSearch) >= 0;
            g.main.style.display = visible ? '' : 'none';
            if (g.follow) g.follow.style.display = visible ? '' : 'none';
        });
    }

    function applySort() {
        if (!tbody) return;
        var ordered;
        if (!activeSort) {
            ordered = rowGroups.slice().sort(function(a, b) {
                return parseInt(a.main.dataset.originalIndex) - parseInt(b.main.dataset.originalIndex);
            });
        } else {
            var col = activeSort.col, dir = activeSort.dir, type = activeSort.type;
            ordered = rowGroups.slice().sort(function(a, b) {
                var va = cellTextAt(a.main, col), vb = cellTextAt(b.main, col);
                var cmp;
                if (type === 'number') {
                    var na = parseFloat(va), nb = parseFloat(vb);
                    var aNaN = isNaN(na), bNaN = isNaN(nb);
                    if (aNaN && bNaN) cmp = 0;
                    else if (aNaN) cmp = 1;        // empties sort last
                    else if (bNaN) cmp = -1;
                    else cmp = na - nb;
                } else {
                    cmp = va.localeCompare(vb, undefined, { numeric: false, sensitivity: 'base' });
                }
                return dir === 'asc' ? cmp : -cmp;
            });
        }
        ordered.forEach(function(g) {
            tbody.appendChild(g.main);
            if (g.follow) tbody.appendChild(g.follow);
        });
    }

    function refreshSortBtnStates() {
        document.querySelectorAll('.ci-sort-btn').forEach(function(btn) {
            var on = activeSort &&
                parseInt(btn.dataset.col) === activeSort.col &&
                btn.dataset.dir === activeSort.dir;
            btn.classList.toggle('ci-sort-active', !!on);
        });
    }

    document.querySelectorAll('.ci-sort-btn').forEach(function(btn) {
        btn.addEventListener('click', function() {
            var col = parseInt(btn.dataset.col);
            var dir = btn.dataset.dir;
            var th = btn.closest('th');
            var type = th ? (th.dataset.colType || 'text') : 'text';
            if (activeSort && activeSort.col === col && activeSort.dir === dir) {
                activeSort = null;       // toggle off
            } else {
                activeSort = { col: col, dir: dir, type: type };
            }
            refreshSortBtnStates();
            applySort();
            refreshReorderState();
        });
    });

    var searchInput = document.getElementById('ci-global-search');
    if (searchInput) {
        searchInput.addEventListener('input', function() {
            activeSearch = searchInput.value.trim().toLowerCase();
            applySearch();
            refreshReorderState();
        });
    }

    // ── Link modal ─────────────────────────────────────────────
    var modal = document.getElementById('link-modal');
    var modalForm = document.getElementById('link-modal-form');
    var modalUrl = document.getElementById('link-modal-url');
    var modalText = document.getElementById('link-modal-text');
    var modalCancel = document.getElementById('link-modal-cancel');
    var modalActiveCell = null;

    function parseLinkValue(v) {
        if (!v) return ['', ''];
        var i = v.indexOf('|');
        if (i < 0) return [v, ''];
        return [v.slice(0, i), v.slice(i + 1)];
    }
    function encodeLinkValue(url, text) {
        if (!url) return '';
        return text ? url + '|' + text : url;
    }
    function renderLinkCellHtml(value) {
        var parsed = parseLinkValue(value);
        var url = parsed[0], text = parsed[1] || lblLinkDefault;
        if (!url) return '';
        var a = document.createElement('a');
        a.href = url;
        a.target = '_blank';
        a.rel = 'noopener';
        a.textContent = text;
        return a.outerHTML;
    }

    if (modalCancel) modalCancel.addEventListener('click', function() {
        modalActiveCell = null;
        if (modal && modal.close) modal.close();
    });
    if (modalForm) modalForm.addEventListener('submit', function(e) {
        e.preventDefault();
        if (!modalActiveCell) { if (modal && modal.close) modal.close(); return; }
        var url = modalUrl.value.trim();
        if (!url) return;
        var text = modalText.value.trim();
        var newValue = encodeLinkValue(url, text);
        var cell = modalActiveCell;
        modalActiveCell = null;
        saveLink(cell, newValue);
        if (modal && modal.close) modal.close();
    });

    function saveLink(cell, newValue) {
        var oldValue = cell.dataset.value || '';
        cell.dataset.value = newValue;
        cell.innerHTML = renderLinkCellHtml(newValue);
        var body = 'row=' + encodeURIComponent(cell.dataset.row)
            + '&col=' + encodeURIComponent(cell.dataset.col)
            + '&value=' + encodeURIComponent(newValue);
        fetch(saveUrl, {
            method: 'POST',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: body,
            credentials: 'same-origin',
        }).then(function(res) {
            if (!res.ok) {
                alert('Save failed (' + res.status + ')');
                cell.dataset.value = oldValue;
                cell.innerHTML = renderLinkCellHtml(oldValue);
            }
        }).catch(function() {
            alert('Save failed (network error)');
            cell.dataset.value = oldValue;
            cell.innerHTML = renderLinkCellHtml(oldValue);
        });
    }

    // ── Cell editing ───────────────────────────────────────────
    // Multiline cells wrap the editable value in a <div class="ci-multiline-value">
    // alongside a label, so the edit/save logic operates on that inner element.
    // Regular cells edit the cell itself.
    function valueEl(cell) {
        return cell.querySelector('.ci-multiline-value') || cell;
    }
    function isMultiline(cell) {
        return cell.classList.contains('ci-multiline-cell');
    }

    // Delegated so rows appended after load are editable too.
    var tableEl = document.querySelector('.ci-input-table');
    if (tableEl) tableEl.addEventListener('dblclick', function(e) {
        var cell = e.target.closest('.ci-cell-editable');
        if (!cell || !tableEl.contains(cell)) return;
        if (cell.classList.contains('ci-cell-editing')) return;
        var colType = cell.dataset.type || 'text';
        if (colType === 'link') {
            modalActiveCell = cell;
            var parsed = parseLinkValue(cell.dataset.value || '');
            modalUrl.value = parsed[0];
            modalText.value = parsed[1];
            if (modal && modal.showModal) modal.showModal();
            return;
        }
        startEdit(cell);
    });

    function startEdit(cell) {
        var target = valueEl(cell);
        var oldValue = target.textContent;
        var colType = cell.dataset.type || 'text';
        cell.classList.add('ci-cell-editing');
        cell.dataset.oldValue = oldValue;
        var control;
        if (colType === 'bool') {
            var parts = lblBool.split(' / ');
            var yes = parts[0] || 'Yes';
            var no = parts[1] || 'No';
            control = document.createElement('select');
            control.className = 'ci-cell ci-cell-select';
            control.innerHTML = '<option value=""></option>'
                + '<option value="' + yes + '">' + yes + '</option>'
                + '<option value="' + no + '">' + no + '</option>';
            control.value = oldValue;
        } else if (colType === 'number') {
            control = document.createElement('input');
            control.type = 'number';
            control.step = 'any';
            control.inputMode = 'decimal';
            control.className = 'ci-cell ci-cell-input';
            control.value = oldValue;
        } else if (isMultiline(cell)) {
            control = document.createElement('textarea');
            control.className = 'ci-cell ci-cell-textarea';
            control.rows = Math.max(3, oldValue.split('\n').length);
            control.value = oldValue;
        } else {
            control = document.createElement('input');
            control.type = 'text';
            control.className = 'ci-cell ci-cell-input';
            control.value = oldValue;
        }
        target.textContent = '';
        target.appendChild(control);
        control.focus();
        if (control.select) control.select();

        var done = false;
        function finish(commit) {
            if (done) return;
            done = true;
            if (commit) {
                save(cell, control.value);
            } else {
                valueEl(cell).textContent = cell.dataset.oldValue || '';
                cell.classList.remove('ci-cell-editing');
                delete cell.dataset.oldValue;
            }
        }
        control.addEventListener('keydown', function(e) {
            // In a textarea Enter inserts a newline; only Ctrl/Cmd+Enter commits.
            if (e.key === 'Enter' && (control.tagName !== 'TEXTAREA' || e.ctrlKey || e.metaKey)) {
                e.preventDefault(); finish(true);
            } else if (e.key === 'Escape') {
                e.preventDefault(); finish(false);
            }
        });
        control.addEventListener('blur', function() { finish(true); });
    }

    function save(cell, newValue) {
        var body = 'row=' + encodeURIComponent(cell.dataset.row)
            + '&col=' + encodeURIComponent(cell.dataset.col)
            + '&value=' + encodeURIComponent(newValue);
        fetch(saveUrl, {
            method: 'POST',
            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
            body: body,
            credentials: 'same-origin',
        }).then(function(res) {
            if (res.ok) {
                valueEl(cell).textContent = newValue;
            } else {
                alert('Save failed (' + res.status + ')');
                valueEl(cell).textContent = cell.dataset.oldValue || '';
            }
        }).catch(function() {
            alert('Save failed (network error)');
            valueEl(cell).textContent = cell.dataset.oldValue || '';
        }).finally(function() {
            cell.classList.remove('ci-cell-editing');
            delete cell.dataset.oldValue;
        });
    }

    // ── Add / delete rows (dynamic inputs only) ────────────────
    // Deleting renumbers the stored CSV, so every data-row in the DOM
    // is rewritten afterwards from the rows' original order. Cell saves
    // address rows by that index, so a stale one would write an edit
    // into the wrong record.
    function setRowIndex(tr, r) {
        tr.dataset.row = r;
        tr.querySelectorAll('[data-row]').forEach(function(el) { el.dataset.row = r; });
    }

    function reindexRows() {
        rowGroups.slice().sort(function(a, b) {
            return parseInt(a.main.dataset.originalIndex) - parseInt(b.main.dataset.originalIndex);
        }).forEach(function(g, i) {
            setRowIndex(g.main, i + 1);          // CSV line 0 is the header
            if (g.follow) setRowIndex(g.follow, i + 1);
            g.main.dataset.originalIndex = i;
        });
    }

    // Only prompt when the row would actually lose data.
    function rowIsEmpty(g) {
        var els = [].slice.call(g.main.querySelectorAll('.ci-cell-editable'));
        if (g.follow) els = els.concat([].slice.call(g.follow.querySelectorAll('.ci-cell-editable')));
        return els.every(function(el) {
            if (el.dataset.type === 'link') return !(el.dataset.value || '');
            return (el.textContent || '').trim() === '';
        });
    }

    if (tableEl) tableEl.addEventListener('click', function(e) {
        var btn = e.target.closest('.ci-row-del');
        if (!btn) return;
        var main = btn.closest('tr.ci-main-row');
        if (!main) return;
        var idx = -1;
        for (var i = 0; i < rowGroups.length; i++) {
            if (rowGroups[i].main === main) { idx = i; break; }
        }
        if (idx < 0) return;
        var g = rowGroups[idx];
        if (!rowIsEmpty(g) && !confirm(lblDeleteRow)) return;
        btn.disabled = true;
        // Queued behind any pending move, so the row index it sends is the
        // one the server holds by then.
        moving.then(function() {
            return fetch(delRowUrl, {
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                body: 'row=' + encodeURIComponent(main.dataset.row),
                credentials: 'same-origin',
            });
        }).then(function(res) {
            if (!res.ok) {
                btn.disabled = false;
                alert(lblRowOpFailed + ' (' + res.status + ')');
                return;
            }
            rowGroups = rowGroups.filter(function(o) { return o !== g; });
            main.remove();
            if (g.follow) g.follow.remove();
            reindexRows();
        }).catch(function() {
            btn.disabled = false;
            alert(lblRowOpFailed);
        });
    });

    var addRowBtn = document.getElementById('ci-add-row-btn');
    if (addRowBtn && tbody) addRowBtn.addEventListener('click', function() {
        addRowBtn.disabled = true;
        // The failure alert covers the request only. By the time the
        // markup comes back the row is already stored, so a later DOM
        // error must not tell the user the row was not added.
        fetch(addRowUrl, { method: 'POST', credentials: 'same-origin' })
            .then(function(res) {
                if (!res.ok) throw new Error(res.status);
                return res.text();
            })
            .catch(function() {
                alert(lblRowOpFailed);
                return null;
            })
            .then(function(html) {
                addRowBtn.disabled = false;
                if (html === null) return;
                tbody.insertAdjacentHTML('beforeend', html);
                var mains = tbody.querySelectorAll('tr.ci-main-row');
                var main = mains[mains.length - 1];
                var follow = tbody.querySelector('tr.ci-multiline-row[data-row="' + main.dataset.row + '"]');
                rowGroups.push({ main: main, follow: follow });
                refreshReorderState();
                // Deliberately not re-sorted or re-filtered: a new blank
                // row must stay visible so it can be filled in.
            });
    });

    // ── Move rows (dynamic inputs only) ────────────────────────
    // A row is dragged by its handle: it jumps between slots as the
    // pointer passes the other rows, and on release the move is posted as
    // (from, to) CSV line indices, then every data-row is renumbered from
    // the new order. Arrow keys on a focused handle move it one place.
    // The handle is a <button>, so the tab swipe leaves it alone, and has
    // `touch-action: none` so a vertical drag is not taken for a scroll.
    //
    // Reordering is the stored order, so it is off while a sort or a search
    // shows the rows in some other order or with gaps.
    var EDGE = 56;  // px from the viewport edge where a drag scrolls the page
    var STEP = 12;  // px scrolled per pointermove there
    var drag = null;  // { g, from, pointerId, handle }
    // Moves go out one after another: each is computed against the order
    // the previous one leaves on the server.
    var moving = Promise.resolve();

    function reorderOff() {
        return !!activeSort || !!activeSearch;
    }

    function refreshReorderState() {
        if (!tableEl) return;
        var off = reorderOff();
        tableEl.classList.toggle('ci-reorder-off', off);
        tableEl.querySelectorAll('.ci-row-handle').forEach(function(h) {
            h.title = off ? lblMoveRowOff : lblMoveRow;
        });
    }

    function groupOf(main) {
        for (var i = 0; i < rowGroups.length; i++) {
            if (rowGroups[i].main === main) return rowGroups[i];
        }
        return null;
    }

    // Row groups in their current on-screen order.
    function domOrder() {
        return Array.prototype.slice.call(tbody.querySelectorAll('tr.ci-main-row'))
            .map(groupOf).filter(Boolean);
    }

    function placeBefore(g, ref) {
        tbody.insertBefore(g.main, ref);
        if (g.follow) tbody.insertBefore(g.follow, ref);
    }

    function lastRowOf(g) {
        return g.follow || g.main;
    }

    // Put group `g` at position `to` of the on-screen order.
    function placeAt(g, to) {
        var others = domOrder().filter(function(o) { return o !== g; });
        placeBefore(g, to < others.length ? others[to].main : null);
    }

    function commitMove(g, from, to) {
        if (from === to) return;
        // The DOM already shows the new order; make it the stored order.
        domOrder().forEach(function(o, i) { o.main.dataset.originalIndex = i; });
        rowGroups = domOrder();
        reindexRows();
        var body = 'from=' + (from + 1) + '&to=' + (to + 1);  // CSV line 0 is the header
        moving = moving.then(function() {
            return fetch(moveRowUrl, {
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                body: body,
                credentials: 'same-origin',
            }).then(function(res) {
                if (!res.ok) throw new Error(res.status);
            });
        }).catch(function() {
            // The page no longer matches what is stored, and any queued
            // move was computed against it: reload to the stored order.
            alert(lblRowOpFailed);
            location.reload();
            return new Promise(function() {});  // drop everything queued after
        });
    }

    function handleOf(target) {
        return target && target.closest ? target.closest('.ci-row-handle') : null;
    }

    if (tableEl && tbody) {
        refreshReorderState();

        tableEl.addEventListener('pointerdown', function(e) {
            var handle = handleOf(e.target);
            if (!handle || e.button !== 0 || drag || reorderOff()) return;
            var g = groupOf(handle.closest('tr.ci-main-row'));
            if (!g) return;
            e.preventDefault();
            handle.setPointerCapture(e.pointerId);
            drag = { g: g, from: domOrder().indexOf(g), pointerId: e.pointerId, handle: handle };
            g.main.classList.add('ci-row-dragging');
            if (g.follow) g.follow.classList.add('ci-row-dragging');
        });

        tableEl.addEventListener('pointermove', function(e) {
            if (!drag || e.pointerId !== drag.pointerId) return;
            e.preventDefault();
            if (e.clientY < EDGE) window.scrollBy(0, -STEP);
            else if (e.clientY > window.innerHeight - EDGE) window.scrollBy(0, STEP);

            var g = drag.g;
            var order = domOrder();
            var i = order.indexOf(g);
            // Swap with a neighbour once the pointer is past its middle,
            // repeatedly, so a fast flick still lands in the right slot.
            while (i > 0) {
                var up = order[i - 1];
                var top = up.main.getBoundingClientRect().top;
                var bottom = lastRowOf(up).getBoundingClientRect().bottom;
                if (e.clientY >= (top + bottom) / 2) break;
                placeBefore(g, up.main);
                order.splice(i, 1); order.splice(i - 1, 0, g); i--;
            }
            while (i < order.length - 1) {
                var down = order[i + 1];
                var dTop = down.main.getBoundingClientRect().top;
                var dBottom = lastRowOf(down).getBoundingClientRect().bottom;
                if (e.clientY <= (dTop + dBottom) / 2) break;
                placeBefore(g, lastRowOf(down).nextSibling);
                order.splice(i, 1); order.splice(i + 1, 0, g); i++;
            }
        });

        function endDrag(e) {
            if (!drag || e.pointerId !== drag.pointerId) return;
            var d = drag;
            drag = null;
            d.g.main.classList.remove('ci-row-dragging');
            if (d.g.follow) d.g.follow.classList.remove('ci-row-dragging');
            if (e.type === 'pointercancel') {
                placeAt(d.g, d.from);
                return;
            }
            commitMove(d.g, d.from, domOrder().indexOf(d.g));
        }
        tableEl.addEventListener('pointerup', endDrag);
        tableEl.addEventListener('pointercancel', endDrag);

        tableEl.addEventListener('keydown', function(e) {
            var handle = handleOf(e.target);
            if (!handle || drag || reorderOff()) return;
            if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
            var g = groupOf(handle.closest('tr.ci-main-row'));
            if (!g) return;
            e.preventDefault();
            var from = domOrder().indexOf(g);
            var to = e.key === 'ArrowUp' ? from - 1 : from + 1;
            if (to < 0 || to >= rowGroups.length) return;
            placeAt(g, to);
            handle.focus();
            commitMove(g, from, to);
        });
    }
})();
