// Swipe left or right anywhere empty to move to the neighbouring nav tab.
//
// The nav bar scrolls sideways on a phone, so reaching a tab means scrolling a
// strip of 5pt text and hitting it; a swipe across the page is the gesture
// every other phone app already uses for the same thing.
//
// The gesture is soft: <main> tracks the finger, so a half-swipe shows where it
// would land and springs back when released, and only a drag past the
// threshold commits. On commit the page slides the rest of the way out while
// the next one loads, and the next one slides in from the opposite edge —
// driven by `data-nav-swipe-in` on <html>, see `nav-swipe-in-*` in core.css.
//
// There is no second page under your finger during the drag: showing one would
// mean prefetching both neighbours on every page load (three server renders per
// navigation) and holding two <main>s in the DOM at once, which every inline
// `getElementById` on a page would then resolve to the wrong one. The gap names
// the tab it is heading for instead of pretending to show it.
//
// This file is inlined into <head>, not the end of <body>: the entry animation
// has to be armed before <main> first paints, or the new page shows up at rest
// for a frame and then jumps offscreen to slide back in. Nothing here touches
// the DOM at load time beyond <html> itself, so running that early is safe.
//
// A swipe only counts when it starts somewhere that has nothing of its own to
// do with it: not on a control, not on a chart, and not inside anything that
// scrolls sideways — a wide table would otherwise change tab instead of
// scrolling.
(function () {
    var SLOP = 8;        // px of travel before the gesture is assigned an axis
    var MIN_X = 60;      // px of travel before it is a swipe at all
    var BIAS = 2;        // how much more sideways than vertical a drag must be
    var FLICK_MS = 300;  // a gesture this short commits on speed, not distance
    var FLICK_X = 40;
    var EXIT_MS = 220;   // keep in step with the entry animation in core.css
    var BACK_MS = 180;
    var FADE_MS = 150;   // the peek label fading out once the gesture is over
    var RESIST = 4;      // how much a drag towards a missing neighbour is damped
    var SKIP = 'input, select, textarea, button, a, canvas, svg, label,' +
        ' [contenteditable], [data-no-swipe]';

    // Arm the entry animation for the page we have just landed on. This runs
    // before <main> exists, which is the whole reason the direction travels in
    // sessionStorage rather than on an element.
    try {
        var entering = sessionStorage.getItem('navSwipeIn');
        if (entering) {
            sessionStorage.removeItem('navSwipeIn');
            document.documentElement.setAttribute('data-nav-swipe-in', entering);
        }
    } catch (e) { /* storage refused: no entry animation, nothing else lost */ }

    // A finished `animation-fill-mode: both` outranks the inline transform a
    // drag writes, so leaving the attribute in place would freeze the next
    // swipe on this page. Drop it the moment the entry animation is over — and
    // again at the start of a drag, for the paths where it never ran.
    document.addEventListener('animationend', function (e) {
        if (e.animationName.indexOf('nav-swipe-in') === 0) disarm();
    });

    function disarm() {
        document.documentElement.removeAttribute('data-nav-swipe-in');
    }

    function tabs() {
        var links = Array.prototype.slice.call(document.querySelectorAll('nav a'));
        var stops = [], hrefs = [];
        links.forEach(function (a) {
            if (a.classList.contains('brand') || a.classList.contains('nav-right')) return;
            var href = a.getAttribute('href');
            // An app may list the same destination twice (its own name and its
            // first tab); they are one stop, not two.
            if (!href || hrefs.indexOf(href) >= 0) return;
            hrefs.push(href);
            stops.push({ href: href, label: a.textContent });
        });
        return stops;
    }

    function currentIndex(stops) {
        var active = document.querySelector('nav a.active');
        if (!active) return -1;
        var href = active.getAttribute('href');
        for (var i = 0; i < stops.length; i++) {
            if (stops[i].href === href) return i;
        }
        return -1;
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

    function reducedMotion() {
        return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    }

    // Past this much travel the drag commits on release.
    function threshold() {
        return Math.max(MIN_X, window.innerWidth * 0.22);
    }

    var peek = null;

    function peekEl() {
        if (!peek) {
            peek = document.createElement('div');
            peek.className = 'nav-swipe-peek';
            document.body.appendChild(peek);
        }
        return peek;
    }

    function fadePeek() {
        if (!peek) return;
        // The drag sets opacity every frame, so it turns the transition off
        // while it owns the label; it is only wanted for this last step.
        peek.style.transition = 'opacity ' + FADE_MS + 'ms var(--ease)';
        peek.style.opacity = 0;
    }

    var startX = null, startY = null, startedAt = 0;
    var axis = null;           // null until the gesture picks 'x' or 'y'
    var soft = true;           // false when the reader has asked for less motion
    var main = null, stops = null, here = -1;

    // Swiping left pulls the next tab in from the right, the way the tab strip
    // itself moves. Returns null at either end of the strip.
    function neighbour(dx) {
        if (!stops) return null;
        var i = here + (dx < 0 ? 1 : -1);
        return (i >= 0 && i < stops.length) ? stops[i] : null;
    }

    function reset() {
        if (main) main.classList.remove('nav-swipe-dragging');
        startX = null;
        startY = null;
        axis = null;
        soft = true;
        stops = null;
        here = -1;
        main = null;
    }

    function springBack() {
        if (main && axis === 'x' && soft) {
            main.style.transition = 'transform ' + BACK_MS + 'ms var(--ease)';
            main.style.transform = '';
        }
        fadePeek();
        reset();
    }

    function go(stop, dir) {
        var el = main;
        reset();
        if (reducedMotion()) {
            window.location.href = stop.href;
            return;
        }
        try { sessionStorage.setItem('navSwipeIn', dir); } catch (e) { /* no entry animation */ }
        // Navigate and animate together rather than one after the other: the
        // browser holds this page on screen until the next one is ready to
        // paint, so the slide covers the round trip instead of following it.
        el.style.transition = 'transform ' + EXIT_MS + 'ms var(--ease), opacity ' + EXIT_MS + 'ms var(--ease)';
        el.style.transform = 'translateX(' + (dir === 'next' ? '-100%' : '100%') + ')';
        el.style.opacity = '0';
        fadePeek();
        window.location.href = stop.href;
    }

    document.addEventListener('touchstart', function (e) {
        reset();
        if (e.touches.length !== 1) return;
        if (!window.matchMedia('(max-width: 640px)').matches) return;
        var t = e.target;
        if (t.closest && (t.closest(SKIP) || scrollsSideways(t))) return;
        main = document.querySelector('main');
        if (!main) return;
        disarm();
        startX = e.touches[0].clientX;
        startY = e.touches[0].clientY;
        startedAt = Date.now();
    }, { passive: true });

    // Not passive, because once the gesture is known to be horizontal it has to
    // be taken away from the browser. Nothing is prevented before that, so a
    // vertical scroll still starts on the very first move — the axis is decided
    // once, at SLOP, and never revisited.
    document.addEventListener('touchmove', function (e) {
        if (startX === null) return;
        if (e.touches.length !== 1) { springBack(); return; }
        var dx = e.touches[0].clientX - startX;
        var dy = e.touches[0].clientY - startY;

        if (axis === null) {
            if (Math.abs(dx) < SLOP && Math.abs(dy) < SLOP) return;
            // Deliberately biased towards leaving the gesture alone. Claiming a
            // scroll by mistake makes every page on the phone stutter; letting
            // a lazy diagonal swipe through as a scroll costs one retry.
            if (Math.abs(dx) <= Math.abs(dy) * BIAS) { reset(); return; }
            stops = tabs();
            here = currentIndex(stops);
            if (here < 0) { reset(); return; }
            axis = 'x';
            // Asked for less motion: the gesture still commits, it just stops
            // being animated on the way. The page keeps its resting position
            // for the whole drag and the tab changes on release.
            soft = !reducedMotion();
            if (soft) {
                main.classList.add('nav-swipe-dragging');
                main.style.transition = 'none';
            }
        }

        // The browser does not get this gesture back, animated or not.
        e.preventDefault();
        if (!soft) return;
        var next = neighbour(dx);
        // Nothing over there to go to: let it move, but only enough to say so.
        var shift = next ? dx : dx / RESIST;
        main.style.transform = 'translateX(' + shift + 'px)';
        var el = peekEl();
        el.style.transition = 'none';
        if (next) {
            el.textContent = next.label;
            el.setAttribute('data-side', dx < 0 ? 'right' : 'left');
            el.style.opacity = Math.min(Math.abs(dx) / threshold(), 1);
        } else {
            el.style.opacity = 0;
        }
    }, { passive: false });

    document.addEventListener('touchend', function (e) {
        if (startX === null || axis !== 'x' || e.changedTouches.length !== 1) {
            springBack();
            return;
        }
        var dx = e.changedTouches[0].clientX - startX;
        var next = neighbour(dx);
        var flick = Date.now() - startedAt < FLICK_MS && Math.abs(dx) >= FLICK_X;
        if (next && (Math.abs(dx) >= threshold() || flick)) {
            go(next, dx < 0 ? 'next' : 'prev');
        } else {
            springBack();
        }
    }, { passive: true });

    document.addEventListener('touchcancel', springBack, { passive: true });

    // Coming back through the bfcache restores the page exactly as it was left,
    // which after a commit is mid-exit: <main> offscreen at opacity 0.
    window.addEventListener('pageshow', function (e) {
        if (!e.persisted) return;
        var el = document.querySelector('main');
        if (el) {
            el.style.transition = '';
            el.style.transform = '';
            el.style.opacity = '';
        }
        fadePeek();
    });
})();
