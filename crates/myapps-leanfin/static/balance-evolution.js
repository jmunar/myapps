// Balance tab: one line, one point per period, clicking a point lists the
// transactions that moved the balance during it.
//
// Kept out of the Rust source so neither the braces nor the quotes need
// escaping; the page inlines it into a `<script>`.
(function () {
    var cfg = document.getElementById('balance-controls').dataset;
    var basePath = cfg.base;
    var balanceChart = null;

    function selectEl() {
        return document.querySelector('#balance-controls select');
    }

    // Each point is the END of a bucket; the payload carries the matching
    // start, so the transactions behind a point need no arithmetic here.
    function onPick(p, index) {
        if (index < 0 || index >= p.dates.length) return;
        window.loadBalanceTxn(p.accountId, p.starts[index], p.dates[index]);
    }

    window.updateBalanceChart = function (p) {
        var canvas = document.getElementById('balance-canvas');
        var emptyEl = document.getElementById('balance-empty');
        if (p.dates.length === 0) {
            canvas.parentElement.style.display = 'none';
            emptyEl.style.display = '';
            return;
        }
        canvas.parentElement.style.display = '';
        emptyEl.style.display = 'none';

        var onClick = function (evt, elems) {
            if (elems.length > 0) onPick(p, elems[0].index);
        };

        if (balanceChart) {
            balanceChart.data.labels = p.dates;
            balanceChart.data.datasets[0].data = p.values;
            balanceChart.options.onClick = onClick;
            balanceChart.update();
        } else {
            balanceChart = new Chart(canvas, {
                type: 'line',
                data: {
                    labels: p.dates,
                    datasets: [{
                        data: p.values,
                        borderColor: '#1A6B5A',
                        backgroundColor: 'rgba(26,107,90,0.15)',
                        fill: true,
                        tension: 0.3,
                        pointRadius: 3,
                        pointHoverRadius: 5
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: {
                        legend: { display: false },
                        tooltip: {
                            callbacks: {
                                label: function (ctx) {
                                    return ctx.parsed.y.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
                                }
                            }
                        }
                    },
                    scales: {
                        x: { ticks: { maxRotation: 45, font: { size: 11 } } },
                        y: { ticks: { callback: function (v) { return v.toLocaleString(); } } }
                    },
                    onClick: onClick
                }
            });
        }
    };

    window.showBalanceEmpty = function (msg) {
        document.getElementById('balance-canvas').parentElement.style.display = 'none';
        var el = document.getElementById('balance-empty');
        el.innerHTML = '<p></p>';
        el.firstChild.textContent = msg;
        el.style.display = '';
    };

    // The window selector feeds the hidden inputs the htmx request includes,
    // then re-triggers it — the account picker owns the request.
    window.balanceWindowChanged = function (win, current) {
        document.getElementById('balance-window').value = win;
        document.getElementById('balance-current').value = current ? '1' : '0';
        htmx.trigger(selectEl(), 'change');
        document.getElementById('balance-txn-card').style.display = 'none';
    };

    window.loadBalanceTxn = function (accountId, dateFrom, dateTo) {
        var url = basePath + '/leanfin/transactions?date_from=' + dateFrom + '&date_to=' + dateTo;
        if (accountId) url += '&account_id=' + accountId;
        var card = document.getElementById('balance-txn-card');
        card.style.display = '';
        document.getElementById('balance-txn-date').textContent =
            dateFrom === dateTo ? dateFrom : dateFrom + ' → ' + dateTo;
        htmx.ajax('GET', url, '#balance-txn-table');
    };
})();
