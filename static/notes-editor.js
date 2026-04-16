(function() {
    var editor = document.getElementById('notes-editor');
    var textarea = document.getElementById('notes-raw');
    var form = document.getElementById('notes-form');
    var BASE = editor.getAttribute('data-base') || '';
    var NOTE_ID = editor.getAttribute('data-note-id');
    var WHISPER = editor.getAttribute('data-whisper') === 'true';
    var T_DICTATING = editor.getAttribute('data-t-dictating') || 'Recording…';
    var T_TRANSCRIBING = editor.getAttribute('data-t-transcribing') || 'Transcribing…';

    // ── Sync editor HTML → markdown textarea ─────────────
    function syncToTextarea() {
        ensureTrailingParagraph();
        textarea.value = htmlToMarkdown(editor);
    }

    function htmlToMarkdown(el) {
        var md = '';
        for (var i = 0; i < el.childNodes.length; i++) {
            var node = el.childNodes[i];
            if (node.nodeType === 3) {
                md += node.textContent;
                continue;
            }
            if (node.nodeType !== 1) continue;
            var tag = node.tagName.toLowerCase();
            switch (tag) {
                case 'h1': md += '# ' + getInlineText(node) + '\n'; break;
                case 'h2': md += '## ' + getInlineText(node) + '\n'; break;
                case 'h3': md += '### ' + getInlineText(node) + '\n'; break;
                case 'p':
                    var t = getInlineText(node);
                    md += (t || '') + '\n';
                    break;
                case 'blockquote': md += '> ' + getInlineText(node) + '\n'; break;
                case 'hr': md += '---\n'; break;
                case 'ul':
                    for (var li = 0; li < node.children.length; li++) {
                        var liEl = node.children[li];
                        var cb = liEl.querySelector('input[type="checkbox"]');
                        if (cb) {
                            md += (cb.checked ? '- [x] ' : '- [ ] ') + getInlineText(liEl) + '\n';
                        } else {
                            md += '- ' + getInlineText(liEl) + '\n';
                        }
                    }
                    break;
                case 'ol':
                    for (var li = 0; li < node.children.length; li++) {
                        md += (li + 1) + '. ' + getInlineText(node.children[li]) + '\n';
                    }
                    break;
                case 'pre':
                    var code = node.querySelector('code');
                    var lang = node.getAttribute('data-lang') || '';
                    md += '```' + lang + '\n' + (code ? code.textContent : node.textContent) + '\n```\n';
                    break;
                case 'br': md += '\n'; break;
                default: md += getInlineText(node) + '\n';
            }
        }
        return md.replace(/\n\n\n+/g, '\n\n').trim();
    }

    function getInlineText(node) {
        var text = '';
        for (var i = 0; i < node.childNodes.length; i++) {
            var ch = node.childNodes[i];
            if (ch.nodeType === 3) { text += ch.textContent; continue; }
            if (ch.nodeType !== 1) continue;
            var tag = ch.tagName.toLowerCase();
            if (tag === 'input') continue;
            if (tag === 'strong' || tag === 'b') text += '**' + getInlineText(ch) + '**';
            else if (tag === 'em' || tag === 'i') text += '*' + getInlineText(ch) + '*';
            else if (tag === 'code') text += '`' + ch.textContent + '`';
            else if (tag === 'a') text += '[' + getInlineText(ch) + '](' + (ch.getAttribute('href') || '') + ')';
            else if (tag === 'br') text += '\n';
            else text += getInlineText(ch);
        }
        return text;
    }

    // Normalize text: replace &nbsp; (0xA0) with regular space
    function norm(s) { return s.replace(/\u00A0/g, ' '); }

    // Find the block-level parent of the current selection
    function currentBlock() {
        var sel = window.getSelection();
        if (!sel.rangeCount) return null;
        var node = sel.anchorNode;
        var block = node.nodeType === 1 ? node : node.parentElement;
        while (block && block !== editor && !isBlockElement(block)) {
            block = block.parentElement;
        }
        if (!block || block === editor) return null;
        return block;
    }

    // ── Live Markdown input handling ─────────────────────
    editor.addEventListener('input', function(e) {
        // List auto-creation: convert "- ", "* ", "1. " as soon as typed
        var block = currentBlock();
        // Also handle bare text nodes directly in the editor (no wrapping <p>)
        if (!block) {
            var sel0 = window.getSelection();
            if (sel0.rangeCount && sel0.anchorNode && sel0.anchorNode.nodeType === 3 && sel0.anchorNode.parentNode === editor) {
                // Wrap the text node in a <p> first so the rest of the logic works
                var bare = sel0.anchorNode;
                var wrapper = document.createElement('p');
                bare.parentNode.insertBefore(wrapper, bare);
                wrapper.appendChild(bare);
                block = wrapper;
            }
        }
        if (block && (block.tagName === 'P' || block.tagName === 'DIV')) {
            var text = norm(block.textContent);
            // Headings: "# ", "## ", "### " (exactly, nothing else)
            var hMatch = text.match(/^(#{1,3})\s$/);
            if (hMatch) {
                var level = hMatch[1].length;
                var heading = document.createElement('h' + level);
                heading.innerHTML = '<br>';
                block.replaceWith(heading);
                setCursorAt(heading, 0);
                syncToTextarea();
                return;
            }
            // Unordered list: "- " or "* " (exactly, nothing else)
            if (text === '- ' || text === '* ') {
                var ul = document.createElement('ul');
                var li = document.createElement('li');
                li.innerHTML = '<br>';
                ul.appendChild(li);
                block.replaceWith(ul);
                setCursorAt(li, 0);
                syncToTextarea();
                return;
            }
            // Ordered list: "1. ", "2. ", etc. (exactly, nothing else)
            var olMatch = text.match(/^(\d+)\.\s$/);
            if (olMatch) {
                var ol = document.createElement('ol');
                ol.setAttribute('start', olMatch[1]);
                var li = document.createElement('li');
                li.innerHTML = '<br>';
                ol.appendChild(li);
                block.replaceWith(ol);
                setCursorAt(li, 0);
                syncToTextarea();
                return;
            }
        }

        // Task checkbox: user typed "[ ] " or "[x] " at the start of a plain <li>
        // (typically right after "- " created the list). Convert the li into
        // a task item with a real checkbox input.
        if (block && block.tagName === 'LI' && !block.classList.contains('notes-task-item')) {
            var firstChild = block.firstChild;
            if (firstChild && firstChild.nodeType === 3) {
                var firstText = norm(firstChild.textContent);
                var cbMatch = firstText.match(/^\[([ xX])\] /);
                if (cbMatch) {
                    firstChild.textContent = firstChild.textContent.substring(cbMatch[0].length);
                    block.classList.add('notes-task-item');
                    var cb = document.createElement('input');
                    cb.type = 'checkbox';
                    cb.setAttribute('contenteditable', 'false');
                    if (cbMatch[1] === 'x' || cbMatch[1] === 'X') {
                        cb.checked = true;
                        cb.setAttribute('checked', '');
                    }
                    block.insertBefore(cb, firstChild);
                    var range = document.createRange();
                    range.setStart(firstChild, 0);
                    range.collapse(true);
                    var sel = window.getSelection();
                    sel.removeAllRanges();
                    sel.addRange(range);
                    syncToTextarea();
                    return;
                }
            }
        }

        // Inline backtick → <code>: when user types closing backtick
        if (e.data === '`') {
            var sel = window.getSelection();
            if (sel.rangeCount) {
                var range = sel.getRangeAt(0);
                var textNode = range.startContainer;
                if (textNode.nodeType === 3) {
                    var content = textNode.textContent;
                    var cursorPos = range.startOffset;
                    // Look for pattern: `text` (opening backtick, content, closing backtick at cursor)
                    var before = content.substring(0, cursorPos);
                    var openIdx = before.lastIndexOf('`', cursorPos - 2);
                    if (openIdx >= 0 && openIdx < cursorPos - 1) {
                        var inner = before.substring(openIdx + 1, cursorPos - 1);
                        if (inner.length > 0) {
                            var beforeText = content.substring(0, openIdx);
                            var afterText = content.substring(cursorPos);

                            var parent = textNode.parentNode;
                            var frag = document.createDocumentFragment();
                            if (beforeText) frag.appendChild(document.createTextNode(beforeText));
                            var codeEl = document.createElement('code');
                            codeEl.textContent = inner;
                            frag.appendChild(codeEl);
                            // Add a zero-width space after so cursor can escape the code element
                            var afterNode = document.createTextNode('\u200B' + afterText);
                            frag.appendChild(afterNode);
                            parent.replaceChild(frag, textNode);

                            // Place cursor after the code element
                            var newRange = document.createRange();
                            newRange.setStart(afterNode, 1);
                            newRange.collapse(true);
                            sel.removeAllRanges();
                            sel.addRange(newRange);
                        }
                    }
                }
            }
        }
        syncToTextarea();
    });

    editor.addEventListener('keydown', function(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
            var sel = window.getSelection();
            if (!sel.rangeCount) return;
            var node = sel.anchorNode;
            var block = node.nodeType === 1 ? node : node.parentElement;
            while (block && block !== editor && !isBlockElement(block)) {
                block = block.parentElement;
            }
            if (!block || block === editor) return;

            // Inside code blocks, allow normal Enter
            if (block.tagName === 'PRE' || block.closest('pre')) return;

            var text = block.textContent;

            // Heading conversion
            if (block.tagName === 'P' || block.tagName === 'DIV') {
                var m = text.match(/^(#{1,3})\s+(.*)/);
                if (m) {
                    e.preventDefault();
                    var level = m[1].length;
                    var heading = document.createElement('h' + level);
                    heading.textContent = m[2];
                    block.replaceWith(heading);
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    heading.insertAdjacentElement('afterend', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }

                // Horizontal rule
                if (text.trim() === '---' || text.trim() === '***') {
                    e.preventDefault();
                    var hr = document.createElement('hr');
                    block.replaceWith(hr);
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    hr.insertAdjacentElement('afterend', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }

                // Blockquote
                if (text.match(/^>\s+(.*)/)) {
                    e.preventDefault();
                    var bq = document.createElement('blockquote');
                    bq.textContent = text.substring(2);
                    block.replaceWith(bq);
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    bq.insertAdjacentElement('afterend', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }

                // Code block
                if (text.match(/^```/)) {
                    e.preventDefault();
                    var lang = text.substring(3).trim();
                    var pre = document.createElement('pre');
                    pre.className = 'notes-code-block';
                    if (lang) pre.setAttribute('data-lang', lang);
                    var code = document.createElement('code');
                    code.textContent = '';
                    pre.appendChild(code);
                    block.replaceWith(pre);
                    setCursorAt(code, 0);
                    syncToTextarea();
                    return;
                }

                // Unordered list (- or *)
                var ulMatch = text.match(/^[-*]\s+(.*)/);
                if (ulMatch) {
                    e.preventDefault();
                    var ul = document.createElement('ul');
                    var li = document.createElement('li');
                    li.textContent = ulMatch[1];
                    ul.appendChild(li);
                    block.replaceWith(ul);
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    ul.insertAdjacentElement('afterend', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }

                // Ordered list (1. text, 2. text, etc.)
                var olMatch = text.match(/^(\d+)\.\s+(.*)/);
                if (olMatch) {
                    e.preventDefault();
                    var ol = document.createElement('ol');
                    var li = document.createElement('li');
                    li.textContent = olMatch[2];
                    ol.appendChild(li);
                    block.replaceWith(ol);
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    ol.insertAdjacentElement('afterend', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }
            }

            // Enter at beginning of a heading → insert empty paragraph before it
            if (/^H[1-6]$/.test(block.tagName)) {
                var sel2 = window.getSelection();
                var range2 = sel2.getRangeAt(0);
                // Check if cursor is at position 0
                if (range2.startOffset === 0 && (range2.startContainer === block || range2.startContainer === block.firstChild)) {
                    e.preventDefault();
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    block.insertAdjacentElement('beforebegin', p);
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }
            }

            // In list items, create new li or exit list on empty
            if (block.tagName === 'LI') {
                if (text.trim() === '') {
                    e.preventDefault();
                    var list = block.parentElement;
                    var p = document.createElement('p');
                    p.innerHTML = '<br>';
                    list.insertAdjacentElement('afterend', p);
                    block.remove();
                    if (list.children.length === 0) list.remove();
                    setCursorAt(p, 0);
                    syncToTextarea();
                    return;
                }
                // Non-empty task item: create a new (empty) task item below,
                // carrying any content that was after the cursor. Without
                // this, the browser's default Enter produces a plain <li>
                // with no checkbox and places the cursor awkwardly.
                if (block.classList.contains('notes-task-item')) {
                    e.preventDefault();
                    var sel = window.getSelection();
                    var range = sel.getRangeAt(0);
                    var tailRange = document.createRange();
                    tailRange.setStart(range.endContainer, range.endOffset);
                    tailRange.setEnd(block, block.childNodes.length);
                    var tail = tailRange.extractContents();

                    var newLi = document.createElement('li');
                    newLi.className = 'notes-task-item';
                    var newCb = document.createElement('input');
                    newCb.type = 'checkbox';
                    newCb.setAttribute('contenteditable', 'false');
                    newLi.appendChild(newCb);
                    newLi.appendChild(tail);
                    // Make sure there's a text node right after the checkbox
                    // so the cursor has a valid offset-0 landing spot.
                    if (!newLi.childNodes[1] || newLi.childNodes[1].nodeType !== 3) {
                        newLi.insertBefore(document.createTextNode(''), newLi.childNodes[1] || null);
                    }
                    block.insertAdjacentElement('afterend', newLi);

                    var newRange = document.createRange();
                    newRange.setStart(newLi.childNodes[1], 0);
                    newRange.collapse(true);
                    sel.removeAllRanges();
                    sel.addRange(newRange);
                    syncToTextarea();
                    return;
                }
            }
        }

        // Ctrl/Cmd+S to save
        if ((e.ctrlKey || e.metaKey) && e.key === 's') {
            e.preventDefault();
            syncToTextarea();
            form.submit();
        }

        // Tab inside code blocks: insert spaces
        if (e.key === 'Tab' && editor.querySelector('pre:focus-within')) {
            e.preventDefault();
            document.execCommand('insertText', false, '    ');
        }
    });

    // ── Exit code blocks with Enter after empty line ─────
    editor.addEventListener('keydown', function(e) {
        if (e.key === 'Enter') {
            var sel = window.getSelection();
            if (!sel.rangeCount) return;
            var node = sel.anchorNode;
            var pre = node.nodeType === 1 ? node.closest('pre') : node.parentElement ? node.parentElement.closest('pre') : null;
            if (!pre) return;
            var code = pre.querySelector('code') || pre;
            var lines = code.textContent.split('\n');
            if (lines.length >= 2 && lines[lines.length - 1] === '' && lines[lines.length - 2] === '') {
                e.preventDefault();
                code.textContent = lines.slice(0, -1).join('\n');
                var p = document.createElement('p');
                p.innerHTML = '<br>';
                pre.insertAdjacentElement('afterend', p);
                setCursorAt(p, 0);
                syncToTextarea();
            }
        }
    });

    function isBlockElement(el) {
        return /^(P|H[1-6]|PRE|BLOCKQUOTE|UL|OL|LI|HR|DIV)$/.test(el.tagName);
    }

    // Ensure the editor always has a trailing <p> so the user can escape
    // block elements (code blocks, lists, blockquotes, etc.) at the end.
    function ensureTrailingParagraph() {
        var last = editor.lastElementChild;
        if (!last || last.tagName !== 'P') {
            var p = document.createElement('p');
            p.innerHTML = '<br>';
            editor.appendChild(p);
        }
    }

    function setCursorAt(el, offset) {
        var range = document.createRange();
        var sel = window.getSelection();
        if (el.childNodes.length > 0) {
            range.setStart(el.childNodes[0] || el, offset);
        } else {
            range.setStart(el, 0);
        }
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
    }

    // ── Task checkbox toggle ─────────────────────────────
    // Clicking a checkbox toggles it natively; mirror the checked property
    // to the attribute so the serialized HTML reflects the new state, then
    // sync to markdown and save immediately (before the 30s auto-save).
    editor.addEventListener('change', function(e) {
        var t = e.target;
        if (!t || t.tagName !== 'INPUT' || t.type !== 'checkbox') return;
        if (t.checked) t.setAttribute('checked', ''); else t.removeAttribute('checked');
        syncToTextarea();
        var formData = new FormData(form);
        fetch(form.action, {
            method: 'POST',
            body: new URLSearchParams(formData)
        }).catch(function() {});
    });

    // Sync before submit
    form.addEventListener('submit', function() {
        syncToTextarea();
    });

    // Auto-save every 30s
    setInterval(function() {
        syncToTextarea();
        var formData = new FormData(form);
        fetch(form.action, {
            method: 'POST',
            body: new URLSearchParams(formData)
        }).catch(function() {});
    }, 30000);

    // Ensure trailing paragraph on initial load
    ensureTrailingParagraph();

    // ── Voice dictation ─────────────────────────────────
    if (WHISPER) {
        var dictBtn = document.getElementById('notes-dictate-btn');
        if (dictBtn) {
            var dictState = 'idle';
            var dictRecorder, dictChunks = [];
            var dictBtnOriginal = dictBtn.innerHTML;

            dictBtn.addEventListener('click', function() {
                if (dictState === 'idle') {
                    navigator.mediaDevices.getUserMedia({ audio: true }).then(function(stream) {
                        dictRecorder = new MediaRecorder(stream);
                        dictChunks = [];
                        dictRecorder.ondataavailable = function(e) { dictChunks.push(e.data); };
                        dictRecorder.onstop = function() {
                            stream.getTracks().forEach(function(t) { t.stop(); });
                            if (dictState === 'transcribing') {
                                dictBtn.textContent = T_TRANSCRIBING;
                                var blob = new Blob(dictChunks, { type: 'audio/webm' });
                                var fd = new FormData();
                                fd.append('audio', blob, 'dictate.webm');
                                fetch(BASE + '/notes/' + NOTE_ID + '/dictate', { method: 'POST', body: fd })
                                    .then(function(r) { return r.text(); })
                                    .then(function(text) {
                                        editor.focus();
                                        document.execCommand('insertText', false, text);
                                        syncToTextarea();
                                        dictBtn.innerHTML = dictBtnOriginal;
                                        dictState = 'idle';
                                    })
                                    .catch(function() {
                                        dictBtn.innerHTML = dictBtnOriginal;
                                        dictState = 'idle';
                                    });
                            }
                        };
                        dictRecorder.start();
                        dictState = 'recording';
                        dictBtn.textContent = T_DICTATING;
                        dictBtn.classList.add('recording');
                    }).catch(function() {
                        dictState = 'idle';
                    });
                } else if (dictState === 'recording') {
                    dictState = 'transcribing';
                    dictBtn.classList.remove('recording');
                    dictRecorder.stop();
                }
            });
        }
    }
})();
