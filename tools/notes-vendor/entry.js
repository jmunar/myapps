// Re-export the Yjs + Tiptap surface the Notes editor consumes.
// esbuild bundles this into static/notes-vendor.bundle.js as an IIFE that
// assigns everything to window.NotesYjsTiptap.

export * as Y from 'yjs';
export { IndexeddbPersistence } from 'y-indexeddb';
export * as syncProtocol from 'y-protocols/sync';
export * as encoding from 'lib0/encoding';
export * as decoding from 'lib0/decoding';

export { Editor } from '@tiptap/core';
export { default as StarterKit } from '@tiptap/starter-kit';
export { default as TaskList } from '@tiptap/extension-task-list';
export { default as TaskItem } from '@tiptap/extension-task-item';
export { default as Link } from '@tiptap/extension-link';
export { default as Collaboration } from '@tiptap/extension-collaboration';
export { Markdown } from 'tiptap-markdown';
