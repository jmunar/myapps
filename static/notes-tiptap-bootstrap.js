// Boots the local-first Notes editor.
//
// Relies on `static/notes-vendor.bundle.js` having loaded first (it
// publishes everything on `window.NotesYjsTiptap`). Reads configuration
// from data attributes on `#notes-editor`:
//   data-base       — app base path (e.g. "" or "/myapps")
//   data-client-uuid— stable per-note id used for IndexedDB key + WS room
//   data-ws-url     — full ws:// or wss:// URL of the sync endpoint
//                     (optional; if missing, derived from window.location)
//
// Persists to IndexedDB so edits survive offline. Syncs to the server via
// a custom WebSocket provider that speaks the y-protocols sync exchange.

(function () {
  if (!window.NotesYjsTiptap) {
    console.error('notes-tiptap-bootstrap: notes-vendor.bundle.js did not load');
    return;
  }

  const {
    Y,
    IndexeddbPersistence,
    syncProtocol,
    encoding,
    decoding,
    Editor,
    StarterKit,
    TaskList,
    TaskItem,
    Link,
    Collaboration,
    Markdown,
  } = window.NotesYjsTiptap;

  const MESSAGE_SYNC = 0;

  function defaultWsUrl(basePath, uuid) {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const base = (basePath || '').replace(/\/$/, '');
    return `${proto}//${location.host}${base}/notes/${uuid}/ws`;
  }

  function buildSyncStep1Message(doc) {
    const enc = encoding.createEncoder();
    encoding.writeVarUint(enc, MESSAGE_SYNC);
    syncProtocol.writeSyncStep1(enc, doc);
    return encoding.toUint8Array(enc);
  }

  function buildUpdateMessage(update) {
    const enc = encoding.createEncoder();
    encoding.writeVarUint(enc, MESSAGE_SYNC);
    syncProtocol.writeUpdate(enc, update);
    return encoding.toUint8Array(enc);
  }

  class NotesWsProvider {
    constructor(url, doc) {
      this.url = url;
      this.doc = doc;
      this.ws = null;
      this.shouldReconnect = true;
      this.reconnectDelay = 1000;
      this._onUpdate = (update, origin) => {
        if (origin === this) return; // updates from server, don't echo
        this._send(buildUpdateMessage(update));
      };
      doc.on('update', this._onUpdate);
      this._connect();
    }

    _connect() {
      const ws = new WebSocket(this.url);
      ws.binaryType = 'arraybuffer';
      this.ws = ws;
      ws.onopen = () => {
        this.reconnectDelay = 1000;
        this._send(buildSyncStep1Message(this.doc));
      };
      ws.onmessage = (e) => {
        this._handleIncoming(new Uint8Array(e.data));
      };
      ws.onclose = () => {
        this.ws = null;
        if (this.shouldReconnect) {
          setTimeout(() => this._connect(), this.reconnectDelay);
          this.reconnectDelay = Math.min(this.reconnectDelay * 2, 30000);
        }
      };
      ws.onerror = () => {/* onclose handles reconnect */};
    }

    _send(bytes) {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.ws.send(bytes);
      }
    }

    _handleIncoming(bytes) {
      const decoder = decoding.createDecoder(bytes);
      const messageType = decoding.readVarUint(decoder);
      if (messageType !== MESSAGE_SYNC) return; // ignore awareness/etc

      const encoder = encoding.createEncoder();
      encoding.writeVarUint(encoder, MESSAGE_SYNC);
      syncProtocol.readSyncMessage(decoder, encoder, this.doc, this);
      if (encoding.length(encoder) > 1) {
        this._send(encoding.toUint8Array(encoder));
      }
    }

    destroy() {
      this.shouldReconnect = false;
      this.doc.off('update', this._onUpdate);
      if (this.ws) this.ws.close();
    }
  }

  // ── Boot ──────────────────────────────────────────────────

  const mount = document.getElementById('notes-editor');
  if (!mount) return;

  const clientUuid = mount.dataset.clientUuid;
  if (!clientUuid) {
    console.error('notes-tiptap-bootstrap: missing data-client-uuid');
    return;
  }
  const wsUrl = mount.dataset.wsUrl || defaultWsUrl(mount.dataset.base, clientUuid);

  const ydoc = new Y.Doc();
  const indexeddb = new IndexeddbPersistence(`notes-${clientUuid}`, ydoc);
  const ws = new NotesWsProvider(wsUrl, ydoc);

  const editor = new Editor({
    element: mount,
    extensions: [
      StarterKit.configure({ history: false }), // CRDT replaces undo history
      TaskList,
      TaskItem.configure({ nested: true }),
      Link,
      Collaboration.configure({ document: ydoc, field: 'body' }),
      Markdown.configure({
        html: false,
        tightLists: true,
        bulletListMarker: '-',
        linkify: false,
        breaks: false,
      }),
    ],
  });

  window.notesEditor = { editor, ydoc, ws, indexeddb };
})();
