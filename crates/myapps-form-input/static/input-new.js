// Entry grid on the "new input" page: pick a row set and form type, then fill
// a grid (fixed rows) or add rows freely (dynamic), serialised to CSV on submit.
//
// Everything that used to be interpolated into this file — the row sets, the
// form types and every label — arrives as one JSON blob in #entry-config, so
// the script is the same bytes for every user and language.

(function() {
    var cfg = JSON.parse(document.getElementById('entry-config').textContent);
    var rowSets = cfg.rowSets;
    var formTypes = cfg.formTypes;
    var lblRow = cfg.labels.row;
    var lblSelectHint = cfg.labels.selectHint;
    var lblBool = cfg.labels.bool;
    var lblRemoveRow = cfg.labels.removeRow;
    var lblNoRowsYet = cfg.labels.noRowsYet;
    var lblLinkDefault = cfg.labels.linkDefault;
    var lblLinkAdd = cfg.labels.linkAdd;

    var rsSel = document.getElementById('row_set_id');
    var ftSel = document.getElementById('form_type_id');
    var rsGroup = document.getElementById('row-set-group');
    var rsWarning = document.getElementById('row-set-warning');
    var gridContainer = document.getElementById('grid-container');
    var addRowBtn = document.getElementById('add-row-btn');
    var submitBtn = document.getElementById('submit-btn');
    var csvInput = document.getElementById('csv_data');
    var form = document.getElementById('input-form');

    // dynamic-mode state: array of arrays of strings; built from DOM on submit
    var dynamicRowCount = 0;

    function currentFormType() {
        var ftId = parseInt(ftSel.value);
        return formTypes.find(function(f) { return f.id === ftId; });
    }

    function currentRowSet() {
        var rsId = parseInt(rsSel.value);
        return rowSets.find(function(r) { return r.id === rsId; });
    }

    function colIsMultiline(col) {
        var ct = col.type || col.col_type || 'text';
        return ct === 'text' && col.multiline === true;
    }

    function escapeHtml(s) {
        return String(s)
            .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
    }

    function cellHtml(r, c, colType) {
        if (colType === 'bool') {
            var parts = lblBool.split(' / ');
            var yes = parts[0] || 'Yes';
            var no = parts[1] || 'No';
            return '<td class="ci-col-bool"><select data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-select">'
                + '<option value=""></option><option value="' + yes + '">' + yes + '</option><option value="' + no + '">' + no + '</option></select></td>';
        } else if (colType === 'number') {
            return '<td class="ci-col-number"><input type="number" step="any" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input" inputmode="decimal"></td>';
        } else if (colType === 'link') {
            return '<td class="ci-col-link">'
                + '<input type="hidden" data-r="' + r + '" data-c="' + c + '" class="ci-cell" value="">'
                + '<button type="button" class="ci-link-btn" onclick="window.openLinkModal(this)">' + lblLinkAdd + '</button>'
                + '</td>';
        }
        return '<td><input type="text" data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-input"></td>';
    }

    function multilineRowHtml(r, cols, colspan) {
        var inner = '';
        for (var c = 0; c < cols.length; c++) {
            if (!colIsMultiline(cols[c])) continue;
            inner += '<div class="ci-multiline-cell">'
                + '<label>' + escapeHtml(cols[c].name) + '</label>'
                + '<textarea data-r="' + r + '" data-c="' + c + '" class="ci-cell ci-cell-textarea" rows="3"></textarea>'
                + '</div>';
        }
        if (!inner) return '';
        return '<tr class="ci-multiline-row" data-row="' + r + '"><td colspan="' + colspan + '">' + inner + '</td></tr>';
    }

    function buildFixedGrid(rs, ft) {
        var rows = rs.rows;
        var cols = ft.columns;
        var visibleCols = 1; // row-name column
        var html = '<table class="ci-input-table"><thead><tr><th class="ci-th-pupil">' + lblRow + '</th>';
        for (var i = 0; i < cols.length; i++) {
            if (colIsMultiline(cols[i])) continue;
            html += '<th>' + escapeHtml(cols[i].name) + '</th>';
            visibleCols++;
        }
        html += '</tr></thead><tbody>';
        for (var r = 0; r < rows.length; r++) {
            html += '<tr class="ci-main-row" data-row="' + r + '"><td class="ci-pupil-name">' + escapeHtml(rows[r]) + '</td>';
            for (var c = 0; c < cols.length; c++) {
                if (colIsMultiline(cols[c])) continue;
                var colType = cols[c].type || cols[c].col_type || 'text';
                html += cellHtml(r, c, colType);
            }
            html += '</tr>';
            html += multilineRowHtml(r, cols, visibleCols);
        }
        html += '</tbody></table>';
        gridContainer.innerHTML = html;
    }

    function dynamicRowHtml(r, cols, visibleCols) {
        var html = '<tr class="ci-main-row" data-row="' + r + '">';
        for (var c = 0; c < cols.length; c++) {
            if (colIsMultiline(cols[c])) continue;
            var colType = cols[c].type || cols[c].col_type || 'text';
            html += cellHtml(r, c, colType);
        }
        html += '<td style="padding:0 0.4rem"><button type="button" class="btn-icon btn-icon-danger remove-row-btn" data-row="' + r + '" title="' + lblRemoveRow + '">×</button></td>';
        html += '</tr>';
        html += multilineRowHtml(r, cols, visibleCols);
        return html;
    }

    function visibleColCountDynamic(cols) {
        var count = 1; // remove-button column
        for (var i = 0; i < cols.length; i++) {
            if (!colIsMultiline(cols[i])) count++;
        }
        return count;
    }

    function buildDynamicGrid(ft) {
        var cols = ft.columns;
        if (cols.length === 0) {
            gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
            return;
        }
        var visibleCols = visibleColCountDynamic(cols);
        var html = '<table class="ci-input-table"><thead><tr>';
        for (var i = 0; i < cols.length; i++) {
            if (colIsMultiline(cols[i])) continue;
            html += '<th>' + escapeHtml(cols[i].name) + '</th>';
        }
        html += '<th></th></tr></thead><tbody id="dynamic-rows">';
        html += dynamicRowHtml(0, cols, visibleCols);
        html += '</tbody></table>';
        gridContainer.innerHTML = html;
        dynamicRowCount = 1;
        wireRemoveButtons(cols);
    }

    function wireRemoveButtons(cols) {
        gridContainer.querySelectorAll('.remove-row-btn').forEach(function(btn) {
            btn.onclick = function() {
                var tbody = document.getElementById('dynamic-rows');
                if (!tbody) return;
                var mainRows = tbody.querySelectorAll('tr.ci-main-row');
                if (mainRows.length <= 1) return;
                var r = btn.dataset.row;
                tbody.querySelectorAll('tr[data-row="' + r + '"]').forEach(function(tr) { tr.remove(); });
            };
        });
    }

    function applyMode() {
        var ft = currentFormType();
        if (!ft) {
            gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
            addRowBtn.style.display = 'none';
            return;
        }
        if (ft.fixed_rows) {
            rsGroup.style.display = '';
            rsSel.required = true;
            addRowBtn.style.display = 'none';
            if (rowSets.length === 0) {
                rsWarning.style.display = '';
                gridContainer.innerHTML = '';
                submitBtn.disabled = true;
                return;
            }
            rsWarning.style.display = 'none';
            submitBtn.disabled = false;
            var rs = currentRowSet();
            if (!rs || ft.columns.length === 0) {
                gridContainer.innerHTML = '<p class="text-secondary">' + lblSelectHint + '</p>';
                return;
            }
            buildFixedGrid(rs, ft);
        } else {
            rsGroup.style.display = 'none';
            rsSel.required = false;
            rsWarning.style.display = 'none';
            addRowBtn.style.display = '';
            submitBtn.disabled = false;
            buildDynamicGrid(ft);
        }
    }

    addRowBtn.addEventListener('click', function() {
        var ft = currentFormType();
        if (!ft || ft.columns.length === 0) return;
        var tbody = document.getElementById('dynamic-rows');
        if (!tbody) return;
        var visibleCols = visibleColCountDynamic(ft.columns);
        tbody.insertAdjacentHTML('beforeend', dynamicRowHtml(dynamicRowCount, ft.columns, visibleCols));
        dynamicRowCount++;
        wireRemoveButtons(ft.columns);
    });

    rsSel.addEventListener('change', applyMode);
    ftSel.addEventListener('change', applyMode);
    applyMode();

    form.addEventListener('submit', function(e) {
        var ft = currentFormType();
        if (!ft) return;
        var cols = ft.columns;
        var lines = [];

        if (ft.fixed_rows) {
            var rs = currentRowSet();
            if (!rs) { e.preventDefault(); return; }
            var rows = rs.rows;
            var header = [lblRow];
            for (var i = 0; i < cols.length; i++) header.push(csvEscape(cols[i].name));
            lines.push(header.join(','));
            for (var r = 0; r < rows.length; r++) {
                var row = [csvEscape(rows[r])];
                for (var c = 0; c < cols.length; c++) {
                    var cell = gridContainer.querySelector('[data-r="' + r + '"][data-c="' + c + '"]');
                    row.push(csvEscape(cell ? cell.value : ''));
                }
                lines.push(row.join(','));
            }
        } else {
            var header2 = [];
            for (var i2 = 0; i2 < cols.length; i2++) header2.push(csvEscape(cols[i2].name));
            lines.push(header2.join(','));
            var mainTrs = gridContainer.querySelectorAll('#dynamic-rows tr.ci-main-row');
            mainTrs.forEach(function(tr) {
                var rIdx = tr.dataset.row;
                var rowVals = [];
                for (var c2 = 0; c2 < cols.length; c2++) {
                    // Multiline cells live in a follow-up tr, so look them
                    // up by the (data-r, data-c) attribute pair rather than
                    // restricting the search to the main tr.
                    var cell2 = gridContainer.querySelector('[data-r="' + rIdx + '"][data-c="' + c2 + '"]');
                    rowVals.push(csvEscape(cell2 ? cell2.value : ''));
                }
                lines.push(rowVals.join(','));
            });
        }
        csvInput.value = lines.join('\n');
    });

    function csvEscape(val) {
        if (!val) return '';
        val = String(val);
        if (val.indexOf(',') >= 0 || val.indexOf('"') >= 0 || val.indexOf('\n') >= 0) {
            return '"' + val.replace(/"/g, '""') + '"';
        }
        return val;
    }

    // ── Link modal ─────────────────────────────────────────────
    var modal = document.getElementById('link-modal');
    var modalForm = document.getElementById('link-modal-form');
    var modalUrl = document.getElementById('link-modal-url');
    var modalText = document.getElementById('link-modal-text');
    var modalCancel = document.getElementById('link-modal-cancel');
    var modalActiveBtn = null;

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
    function buttonLabel(text) {
        return text || lblLinkDefault;
    }

    window.openLinkModal = function(btn) {
        modalActiveBtn = btn;
        var hidden = btn.previousElementSibling;
        var current = hidden ? hidden.value : '';
        var parsed = parseLinkValue(current);
        modalUrl.value = parsed[0];
        modalText.value = parsed[1];
        if (modal && modal.showModal) modal.showModal();
    };

    if (modalCancel) modalCancel.addEventListener('click', function() {
        modalActiveBtn = null;
        if (modal && modal.close) modal.close();
    });

    if (modalForm) modalForm.addEventListener('submit', function(e) {
        e.preventDefault();
        if (!modalActiveBtn) { if (modal && modal.close) modal.close(); return; }
        var url = modalUrl.value.trim();
        if (!url) return;
        var text = modalText.value.trim();
        var hidden = modalActiveBtn.previousElementSibling;
        if (hidden) hidden.value = encodeLinkValue(url, text);
        modalActiveBtn.textContent = buttonLabel(text);
        modalActiveBtn = null;
        if (modal && modal.close) modal.close();
    });

    // ── Tabs (manual entry vs CSV upload) ────────────────────────
    var tabBtnManual = document.getElementById('tab-btn-manual');
    var tabBtnCsv = document.getElementById('tab-btn-csv');
    var tabPaneManual = document.getElementById('tab-pane-manual');
    var tabPaneCsv = document.getElementById('tab-pane-csv');
    function activateTab(name) {
        var manual = name === 'manual';
        tabPaneManual.style.display = manual ? '' : 'none';
        tabPaneCsv.style.display = manual ? 'none' : '';
        tabBtnManual.style.borderBottomColor = manual ? 'var(--accent-color, #1A6B5A)' : 'transparent';
        tabBtnManual.style.fontWeight = manual ? '600' : '';
        tabBtnManual.setAttribute('aria-selected', manual ? 'true' : 'false');
        tabBtnCsv.style.borderBottomColor = manual ? 'transparent' : 'var(--accent-color, #1A6B5A)';
        tabBtnCsv.style.fontWeight = manual ? '' : '600';
        tabBtnCsv.setAttribute('aria-selected', manual ? 'false' : 'true');
    }
    tabBtnManual.addEventListener('click', function() { activateTab('manual'); });
    tabBtnCsv.addEventListener('click', function() { activateTab('csv'); });

    // ── CSV form: mirror the row-set visibility logic ────────────
    var csvRsSel = document.getElementById('csv_row_set_id');
    var csvFtSel = document.getElementById('csv_form_type_id');
    var csvRsGroup = document.getElementById('csv-row-set-group');
    var csvRsWarning = document.getElementById('csv-row-set-warning');
    var csvSubmitBtn = document.getElementById('csv-submit-btn');
    var csvFormatHint = document.getElementById('csv-format-hint');
    var lblCsvFormatDynamic = cfg.labels.csvFormatDynamic;
    var lblCsvFormatFixed = cfg.labels.csvFormatFixed;

    function applyCsvMode() {
        var ftId = parseInt(csvFtSel.value);
        var ft = formTypes.find(function(f) { return f.id === ftId; });
        if (!ft) {
            csvSubmitBtn.disabled = true;
            return;
        }
        if (ft.fixed_rows) {
            csvRsGroup.style.display = '';
            csvRsSel.required = true;
            csvFormatHint.textContent = lblCsvFormatFixed;
            if (rowSets.length === 0) {
                csvRsWarning.style.display = '';
                csvSubmitBtn.disabled = true;
                return;
            }
            csvRsWarning.style.display = 'none';
            csvSubmitBtn.disabled = false;
        } else {
            csvRsGroup.style.display = 'none';
            csvRsSel.required = false;
            csvRsWarning.style.display = 'none';
            csvSubmitBtn.disabled = false;
            csvFormatHint.textContent = lblCsvFormatDynamic;
        }
    }
    csvFtSel.addEventListener('change', applyCsvMode);
    applyCsvMode();
})();
