// Time-window selector: one box you swipe up for a longer window and down for
// a shorter one, plus a `+` that adds the period we are currently in.
//
// Kept out of the Rust source so neither the braces nor the quotes need
// escaping; `period::SELECTOR_JS` inlines it into both pages that use it.
(function () {
    var ORDER = ['30d', '10w', '6m', '12m'];
    var SWIPE = 24;   // px of vertical travel before a drag counts as a step

    function notify(root) {
        var fn = window[root.dataset.onchange];
        if (typeof fn === 'function') {
            fn(root.dataset.window, root.dataset.current === '1');
        }
    }

    function setWindow(root, key) {
        if (key === root.dataset.window) return;
        root.dataset.window = key;
        root.querySelector('.lf-window-value').textContent = key;
        root.querySelector('.lf-window-box').setAttribute('aria-valuetext', key);
        notify(root);
    }

    // Positive `delta` lengthens the window. Both ends stop rather than wrap:
    // wrapping from 12m back to 30d on an extra swipe is never what was meant.
    function step(root, delta) {
        var i = ORDER.indexOf(root.dataset.window);
        if (i < 0) i = 0;
        var next = Math.min(ORDER.length - 1, Math.max(0, i + delta));
        setWindow(root, ORDER[next]);
    }

    function toggleCurrent(root, btn) {
        var on = root.dataset.current !== '1';
        root.dataset.current = on ? '1' : '0';
        btn.setAttribute('aria-pressed', on ? 'true' : 'false');
        btn.classList.toggle('lf-window-now-active', on);
        notify(root);
    }

    function init(root) {
        if (root.dataset.wired) return;
        root.dataset.wired = '1';

        var box = root.querySelector('.lf-window-box');

        root.querySelectorAll('.lf-window-step').forEach(function (btn) {
            btn.addEventListener('click', function (e) {
                e.preventDefault();
                e.stopPropagation();
                step(root, btn.dataset.step === 'longer' ? 1 : -1);
            });
        });

        var now = root.querySelector('.lf-window-now');
        if (now) {
            now.addEventListener('click', function (e) {
                e.preventDefault();
                toggleCurrent(root, now);
            });
        }

        box.addEventListener('keydown', function (e) {
            if (e.key === 'ArrowUp' || e.key === 'ArrowRight') {
                e.preventDefault();
                step(root, 1);
            } else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') {
                e.preventDefault();
                step(root, -1);
            }
        });

        box.addEventListener('wheel', function (e) {
            if (Math.abs(e.deltaY) < 1) return;
            e.preventDefault();
            step(root, e.deltaY < 0 ? 1 : -1);
        }, { passive: false });

        // Drag: one step per SWIPE pixels, so a long swipe can cross two
        // windows without lifting a finger. `touch-action: none` on the box
        // keeps the page from scrolling underneath it.
        var startY = null;
        var taken = 0;

        box.addEventListener('pointerdown', function (e) {
            if (e.target.closest('.lf-window-step')) return;
            startY = e.clientY;
            taken = 0;
            box.setPointerCapture(e.pointerId);
        });

        box.addEventListener('pointermove', function (e) {
            if (startY === null) return;
            var steps = Math.trunc((startY - e.clientY) / SWIPE);
            if (steps !== taken) {
                step(root, steps - taken);
                taken = steps;
            }
        });

        function end(e) {
            if (startY === null) return;
            startY = null;
            if (box.hasPointerCapture(e.pointerId)) box.releasePointerCapture(e.pointerId);
        }
        box.addEventListener('pointerup', end);
        box.addEventListener('pointercancel', end);
    }

    window.leanfinInitWindowSelectors = function () {
        document.querySelectorAll('.lf-window').forEach(init);
    };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', window.leanfinInitWindowSelectors);
    } else {
        window.leanfinInitWindowSelectors();
    }
})();
