// Breakdown tab: a time series of one label group's net flow per period, and
// a horizontal bar per label inside it.
//
// Money keeps its statement sign throughout — spending negative, income
// positive — so both charts cross zero rather than inverting anything.
//
// Kept out of the Rust source so neither the braces nor the quotes need
// escaping; the page inlines it into a `<script>`.
(function () {
    var cfg = document.getElementById('breakdown-controls').dataset;
    var basePath = cfg.base;
    var selectGroupMsg = cfg.msgSelectGroup;
    var allRangeMsg = cfg.msgFullRange;

    var selectedGroup = null;
    var currentWindow = cfg.window;
    var includeCurrent = cfg.current === '1';
    var timeChart = null;
    var catChart = null;
    var payload = null;
    var selectedBucket = null;   // index into payload.dates, or null for the whole window

    function activeRange() {
        if (selectedBucket === null) {
            return [payload.starts[0], payload.dates[payload.dates.length - 1]];
        }
        return [payload.starts[selectedBucket], payload.dates[selectedBucket]];
    }

    function totalsPerBucket() {
        return payload.dates.map(function (_, i) {
            return payload.matrix.reduce(function (sum, row) { return sum + row[i]; }, 0);
        });
    }

    // How many whole periods the window actually observed. The period we are
    // living in counts only for the fraction of it that has happened, so a
    // month that is three days old cannot drag the average down.
    function observedPeriods() {
        return payload.weights.reduce(function (a, b) { return a + b; }, 0);
    }

    function average(total) {
        var n = observedPeriods();
        return n > 0 ? total / n : 0;
    }

    // Label totals over the active range, biggest first. Ranking by magnitude
    // rather than by value keeps the largest spend at the top even though
    // spending is the negative side of zero.
    function categoryTotals() {
        var from = selectedBucket === null ? 0 : selectedBucket;
        var to = selectedBucket === null ? payload.dates.length - 1 : selectedBucket;
        return payload.categories.map(function (cat, ci) {
            var sum = 0;
            for (var i = from; i <= to; i++) sum += payload.matrix[ci][i];
            return { id: cat.id, name: cat.name, total: sum };
        }).filter(function (c) { return Math.abs(c.total) > 0.005; })
            .sort(function (a, b) { return Math.abs(b.total) - Math.abs(a.total); });
    }

    function money(v) {
        return v.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    }

    function renderTimeChart() {
        var canvas = document.getElementById('breakdown-canvas');
        var emptyEl = document.getElementById('breakdown-empty');
        canvas.style.display = '';
        canvas.parentElement.style.display = '';
        emptyEl.style.display = 'none';

        var totals = totalsPerBucket();
        var avg = average(totals.reduce(function (a, b) { return a + b; }, 0));

        var data = {
            labels: payload.dates,
            datasets: [{
                label: payload.groupName,
                data: totals,
                backgroundColor: payload.color,
                borderRadius: 4,
                borderSkipped: false,
                order: 1
            }, {
                // A flat reference line, not a series: it exists so a bar can
                // be read as above or below a typical period.
                type: 'line',
                label: payload.avgLabel,
                data: payload.dates.map(function () { return avg; }),
                borderColor: 'rgba(90,90,90,0.9)',
                borderWidth: 2,
                borderDash: [6, 4],
                pointRadius: 0,
                pointHitRadius: 0,
                fill: false,
                order: 0
            }]
        };
        var options = {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: { display: true, labels: { boxWidth: 14, font: { size: 11 } } },
                tooltip: {
                    callbacks: {
                        label: function (ctx) {
                            return ctx.dataset.label + ': ' + money(ctx.parsed.y);
                        }
                    }
                }
            },
            scales: {
                x: { ticks: { maxRotation: 45, font: { size: 11 } }, grid: { display: false } },
                y: { ticks: { callback: function (v) { return v.toLocaleString(); } } }
            },
            onClick: function (evt, elems) {
                // Only the bars select a period; the average line is scenery.
                var bar = elems.filter(function (e) { return e.datasetIndex === 0; })[0];
                if (!bar) return;
                selectedBucket = (selectedBucket === bar.index) ? null : bar.index;
                renderCategoryChart();
            }
        };

        if (timeChart) {
            timeChart.data = data;
            timeChart.options = options;
            timeChart.update();
        } else {
            timeChart = new Chart(canvas, { type: 'bar', data: data, options: options });
        }
    }

    function renderCategoryChart() {
        var totals = categoryTotals();
        var card = document.getElementById('breakdown-categories-card');
        var range = activeRange();
        // Over the whole window the bars are per-period averages, so the
        // heading has to say so — the axis numbers alone would read as totals.
        document.getElementById('breakdown-range').textContent =
            selectedBucket === null
                ? allRangeMsg + ' · ' + payload.avgLabel
                : range[0] + ' → ' + range[1];

        if (totals.length === 0) {
            card.style.display = 'none';
            if (catChart) { catChart.destroy(); catChart = null; }
            return;
        }
        card.style.display = '';

        var perPeriod = selectedBucket === null;
        var values = totals.map(function (c) {
            return perPeriod ? average(c.total) : c.total;
        });

        // One slim bar per label. The rows are tight because the labels are
        // short and the list can run long.
        document.getElementById('breakdown-categories-container').style.height =
            Math.max(120, 26 * totals.length + 56) + 'px';

        var data = {
            labels: totals.map(function (c) { return c.name; }),
            datasets: [{
                label: perPeriod ? payload.avgLabel : payload.groupName,
                data: values,
                backgroundColor: payload.color,
                borderRadius: 4,
                borderSkipped: false,
                barThickness: 14,
                maxBarThickness: 14
            }]
        };
        var options = {
            indexAxis: 'y',
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: { display: false },
                tooltip: { callbacks: { label: function (ctx) { return money(ctx.parsed.x); } } }
            },
            scales: {
                x: { ticks: { callback: function (v) { return v.toLocaleString(); } } },
                y: { grid: { display: false }, ticks: { font: { size: 11 } } }
            },
            onClick: function (evt, elems) {
                if (elems.length === 0) return;
                var cat = totals[elems[0].index];
                var r = activeRange();
                loadTransactions(cat.id, cat.name, r[0], r[1]);
            }
        };

        if (catChart) {
            catChart.data = data;
            catChart.options = options;
            catChart.update();
        } else {
            catChart = new Chart(document.getElementById('breakdown-categories-canvas'),
                { type: 'bar', data: data, options: options });
        }
    }

    window.updateBreakdown = function (next) {
        payload = next;
        selectedBucket = null;
        document.getElementById('breakdown-txn-card').style.display = 'none';
        renderTimeChart();
        renderCategoryChart();
    };

    window.showBreakdownEmpty = function (msg) {
        payload = null;
        selectedBucket = null;
        var canvas = document.getElementById('breakdown-canvas');
        canvas.style.display = 'none';
        // Hide the container too: its fixed height would otherwise leave
        // 300px of blank space above the message.
        canvas.parentElement.style.display = 'none';
        document.getElementById('breakdown-categories-card').style.display = 'none';
        document.getElementById('breakdown-txn-card').style.display = 'none';
        if (timeChart) { timeChart.destroy(); timeChart = null; }
        if (catChart) { catChart.destroy(); catChart = null; }
        var el = document.getElementById('breakdown-empty');
        el.innerHTML = '<p></p>';
        el.firstChild.textContent = msg;
        el.style.display = '';
    };

    // Exactly one group at a time: the pills behave as radio buttons, and
    // clicking the selected one clears the charts.
    document.getElementById('breakdown-controls').addEventListener('click', function (e) {
        var btn = e.target.closest('.label-pill');
        if (!btn) return;
        var id = btn.dataset.groupId;
        document.querySelectorAll('#breakdown-controls .label-pill')
            .forEach(function (b) { b.classList.remove('label-pill-active'); });
        if (selectedGroup === id) {
            selectedGroup = null;
            window.showBreakdownEmpty(selectGroupMsg);
            return;
        }
        selectedGroup = id;
        btn.classList.add('label-pill-active');
        loadChart();
    });

    window.breakdownWindowChanged = function (win, current) {
        currentWindow = win;
        includeCurrent = current;
        if (selectedGroup) loadChart();
    };

    function loadChart() {
        htmx.ajax('GET', basePath + '/leanfin/breakdown/chart?group_id=' + selectedGroup
            + '&window=' + currentWindow
            + '&current=' + (includeCurrent ? '1' : '0'), '#breakdown-data');
    }

    function loadTransactions(labelId, labelName, dateFrom, dateTo) {
        var url = basePath + '/leanfin/transactions?label_ids=' + labelId
            + '&date_from=' + dateFrom + '&date_to=' + dateTo;
        document.getElementById('breakdown-txn-card').style.display = '';
        document.getElementById('breakdown-txn-range').textContent =
            labelName + ' · ' + dateFrom + ' → ' + dateTo;
        htmx.ajax('GET', url, '#breakdown-txn-table');
    }
})();
