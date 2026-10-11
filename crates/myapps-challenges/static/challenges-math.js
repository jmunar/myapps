// Typeset the LaTeX in every [data-challenges-math] element with KaTeX.
//
// katex.min.js and auto-render.min.js are loaded with `defer`, which runs them
// before DOMContentLoaded, so waiting for that event is enough. The text was
// HTML-escaped on the server; auto-render reads text nodes, so it sees the
// original LaTeX. A formula KaTeX cannot parse stays as red source instead of
// stopping the rest of the page. Marking or skipping swaps the next problem in
// with htmx, so whatever a swap brings in is typeset too.

(function() {
    // Passing `delimiters` replaces auto-render's defaults, so everything the
    // datasets use has to be listed. Hendrycks MATH writes display maths as a
    // bare environment (`\begin{align*}` with no `$$` around it); those come
    // after `$$` and `\[` so an environment already inside them is left to
    // the outer delimiter, and before `$` so a `$` inside one is not split.
    var DELIMITERS = [
        { left: '$$', right: '$$', display: true },
        { left: '\\[', right: '\\]', display: true }
    ];
    ['align', 'align*', 'equation', 'equation*', 'gather', 'gather*',
     'alignat', 'alignat*', 'multline', 'multline*'].forEach(function(env) {
        DELIMITERS.push({
            left: '\\begin{' + env + '}',
            right: '\\end{' + env + '}',
            display: true
        });
    });
    DELIMITERS.push(
        { left: '\\(', right: '\\)', display: false },
        { left: '$', right: '$', display: false }
    );

    function typeset(root) {
        if (typeof renderMathInElement !== 'function') return;
        var els = (root || document).querySelectorAll('[data-challenges-math]');
        for (var i = 0; i < els.length; i++) {
            renderMathInElement(els[i], {
                delimiters: DELIMITERS,
                throwOnError: false
            });
        }
    }
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function() { typeset(); });
    } else {
        typeset();
    }
    document.addEventListener('htmx:afterSwap', function(e) {
        typeset(e.detail.target);
    });
})();
