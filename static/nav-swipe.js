// Swipe left or right anywhere empty to move to the neighbouring nav tab.
//
// The nav bar scrolls sideways on a phone, so reaching a tab means scrolling a
// strip of 5pt text and hitting it; a swipe across the page is the gesture
// every other phone app already uses for the same thing.
//
// A swipe only counts when it starts somewhere that has nothing of its own to
// do with it: not on a control, not on a chart, and not inside anything that
// scrolls sideways — a wide table would otherwise change tab instead of
// scrolling.
(function () {
    var MIN_X = 60;      // px of travel before it is a swipe at all
    var MAX_MS = 800;    // slower than this is a drag, not a swipe
    var SKIP = 'input, select, textarea, button, a, canvas, svg, label,' +
        ' [contenteditable], [data-no-swipe]';

    function tabs() {
        var links = Array.prototype.slice.call(document.querySelectorAll('nav a'));
        var hrefs = [];
        links.forEach(function (a) {
            if (a.classList.contains('brand') || a.classList.contains('nav-right')) return;
            var href = a.getAttribute('href');
            // An app may list the same destination twice (its own name and its
            // first tab); they are one stop, not two.
            if (href && hrefs.indexOf(href) < 0) hrefs.push(href);
        });
        return hrefs;
    }

    function scrollsSideways(el) {
        for (var n = el; n && n !== document.body; n = n.parentElement) {
            if (n.scrollWidth > n.clientWidth + 4) {
                var ox = getComputedStyle(n).overflowX;
                if (ox === 'auto' || ox === 'scroll') return true;
            }
        }
        return false;
    }

    var startX = null, startY = null, startedAt = 0;

    document.addEventListener('touchstart', function (e) {
        startX = null;
        if (e.touches.length !== 1) return;
        if (!window.matchMedia('(max-width: 640px)').matches) return;
        var t = e.target;
        if (t.closest && (t.closest(SKIP) || scrollsSideways(t))) return;
        startX = e.touches[0].clientX;
        startY = e.touches[0].clientY;
        startedAt = Date.now();
    }, { passive: true });

    document.addEventListener('touchend', function (e) {
        if (startX === null || e.changedTouches.length !== 1) return;
        var dx = e.changedTouches[0].clientX - startX;
        var dy = e.changedTouches[0].clientY - startY;
        startX = null;
        if (Date.now() - startedAt > MAX_MS) return;
        // Mostly sideways, or it was a scroll that wandered.
        if (Math.abs(dx) < MIN_X || Math.abs(dx) < Math.abs(dy) * 2) return;

        var hrefs = tabs();
        var active = document.querySelector('nav a.active');
        if (!active) return;
        var i = hrefs.indexOf(active.getAttribute('href'));
        if (i < 0) return;

        // Swiping left pulls the next tab in from the right, the way the tab
        // strip itself moves.
        var next = i + (dx < 0 ? 1 : -1);
        if (next < 0 || next >= hrefs.length) return;
        window.location.href = hrefs[next];
    }, { passive: true });
})();
