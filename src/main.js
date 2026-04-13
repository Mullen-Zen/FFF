// Tauri 2.x injects window.__TAURI__ automatically — no npm install needed.
// Plugin APIs (dialog, opener, etc.) are not on window.__TAURI__ in plain HTML;
// call them via invoke with the 'plugin:<name>|<command>' convention instead.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── DOM refs ──────────────────────────────────────────────────────────────────
const folderList    = document.getElementById('folder-list');
const btnAddFolder  = document.getElementById('btn-add-folder');
const btnClearIndex = document.getElementById('btn-clear-index');
const searchInput   = document.getElementById('search-input');
const resultsGrid   = document.getElementById('results-grid');
const statusText    = document.getElementById('status-text');
const statusCount   = document.getElementById('status-count');
const progressBar   = document.getElementById('progress-bar');
const progressCont  = document.getElementById('progress-bar-container');
const browseBar     = document.getElementById('browse-bar');
const currentPath   = document.getElementById('current-path');
const btnUp         = document.getElementById('btn-up');

// ── State ─────────────────────────────────────────────────────────────────────
let currentDir   = null;   // null = search/idle mode
let searchTimer  = null;
let activeFolderEl = null;

// ── Utilities ─────────────────────────────────────────────────────────────────

function extIcon(ext) {
  const map = {
    pdf: '📄', doc: '📝', docx: '📝', odt: '📝',
    xls: '📊', xlsx: '📊', csv: '📊',
    ppt: '📽', pptx: '📽',
    jpg: '🖼', jpeg: '🖼', png: '🖼', gif: '🖼', svg: '🖼', webp: '🖼',
    mp4: '🎬', mkv: '🎬', avi: '🎬', mov: '🎬',
    mp3: '🎵', wav: '🎵', flac: '🎵', ogg: '🎵',
    zip: '🗜', rar: '🗜', '7z': '🗜', tar: '🗜', gz: '🗜',
    exe: '⚙', dll: '⚙', msi: '⚙',
    rs: '🦀', go: '🐹', py: '🐍', js: '📜', ts: '📜',
    html: '🌐', css: '🎨', json: '🗂', toml: '🗂', yaml: '🗂', yml: '🗂',
    md: '📋', txt: '📃', log: '📃',
    sh: '🖥', bat: '🖥', ps1: '🖥',
    c: '⌨', cpp: '⌨', h: '⌨', java: '☕', rb: '💎', kt: '🟣',
    db: '🗃', sqlite: '🗃',
  };
  return map[ext?.toLowerCase()] ?? '📁';
}

function formatBytes(bytes) {
  if (!bytes) return '';
  if (bytes < 1024)        return bytes + ' B';
  if (bytes < 1024 ** 2)   return (bytes / 1024).toFixed(1) + ' KB';
  if (bytes < 1024 ** 3)   return (bytes / 1024 ** 2).toFixed(1) + ' MB';
  return (bytes / 1024 ** 3).toFixed(1) + ' GB';
}

function setStatus(text) {
  statusText.textContent = text;
}

// ── Render helpers ────────────────────────────────────────────────────────────

function renderFileCard(f) {
  const ext  = f.extension ?? '';
  const card = document.createElement('div');
  card.className = 'file-card';

  const tagsHtml = (f.tags ?? [])
    .map(t => `<span class="tag ai">${escHtml(t)}</span>`)
    .join('');

  card.innerHTML = `
    <div class="file-card-header">
      <span class="file-icon">${extIcon(ext)}</span>
      <span class="file-name" title="${escHtml(f.name)}">${escHtml(f.name)}</span>
    </div>
    <div class="file-path" title="${escHtml(f.path)}">${escHtml(f.path)}</div>
    <div class="file-tags">${tagsHtml}</div>
    <div class="file-actions">
      <button data-action="open"   data-path="${escAttr(f.path)}">Open</button>
      <button data-action="reveal" data-path="${escAttr(f.path)}">Reveal</button>
    </div>
  `;
  return card;
}

function renderDirCard(e) {
  const card = document.createElement('div');
  card.className = 'file-card' + (e.is_dir ? ' is-dir' : '');

  const icon = e.is_dir ? '📁' : extIcon(e.name.split('.').pop());
  const meta = e.is_dir ? 'folder' : formatBytes(e.size);

  const actionsHtml = e.is_dir ? '' : `
    <div class="file-actions">
      <button data-action="open"   data-path="${escAttr(e.path)}">Open</button>
      <button data-action="reveal" data-path="${escAttr(e.path)}">Reveal</button>
    </div>`;

  card.innerHTML = `
    <div class="file-card-header">
      <span class="file-icon">${icon}</span>
      <span class="file-name" title="${escHtml(e.name)}">${escHtml(e.name)}</span>
    </div>
    <div class="file-path" title="${escHtml(e.path)}">${escHtml(e.path)}</div>
    <div class="file-tags"><span class="tag">${meta}</span></div>
    ${actionsHtml}
  `;

  if (e.is_dir) {
    card.addEventListener('dblclick', () => browseDir(e.path));
  }
  return card;
}

// Minimal HTML escaping to prevent XSS from filenames/paths
function escHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
function escAttr(s) {
  return String(s).replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── Browse directory ──────────────────────────────────────────────────────────

async function browseDir(path) {
  try {
    currentDir = path;
    currentPath.textContent = path;
    browseBar.classList.add('visible');
    searchInput.value = '';

    const entries = await invoke('browse_directory', { path });
    resultsGrid.innerHTML = '';

    if (!entries.length) {
      resultsGrid.innerHTML = '<div class="empty-state">This folder is empty.</div>';
    } else {
      entries.forEach(e => resultsGrid.appendChild(renderDirCard(e)));
    }
    statusCount.textContent = `${entries.length} item${entries.length !== 1 ? 's' : ''}`;
  } catch (err) {
    setStatus(`Browse error: ${err}`);
  }
}

// ── Search ────────────────────────────────────────────────────────────────────

async function performSearch(query) {
  if (!query.trim()) {
    resultsGrid.innerHTML = '<div class="empty-state">Type to search indexed files by name, path, or tag.</div>';
    statusCount.textContent = '';
    browseBar.classList.remove('visible');
    currentDir = null;
    return;
  }

  browseBar.classList.remove('visible');
  currentDir = null;

  try {
    const results = await invoke('search_files', { query });
    resultsGrid.innerHTML = '';

    if (!results.length) {
      resultsGrid.innerHTML = `<div class="empty-state">No results for "<strong>${escHtml(query)}</strong>".<br>Try indexing more folders first.</div>`;
    } else {
      results.forEach(f => resultsGrid.appendChild(renderFileCard(f)));
    }
    statusCount.textContent = `${results.length} result${results.length !== 1 ? 's' : ''}`;
  } catch (err) {
    setStatus(`Search error: ${err}`);
  }
}

// ── Sidebar folder list ───────────────────────────────────────────────────────

async function loadFolders() {
  try {
    const dirs = await invoke('get_indexed_dirs');
    folderList.innerHTML = '';

    if (!dirs.length) {
      const li = document.createElement('li');
      li.style.cssText = 'font-style:italic;cursor:default;';
      li.textContent = 'No folders yet';
      folderList.appendChild(li);
      return;
    }

    dirs.forEach(d => {
      const li = document.createElement('li');
      li.className = 'indexed-folder-item';

      const label = d.replace(/\\/g, '/').split('/').filter(Boolean).pop() || d;

      li.innerHTML = `
        <span class="folder-icon">📁</span>
        <span class="folder-label" title="${escHtml(d)}">${escHtml(label)}</span>
        <button class="delete-folder-btn">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 6h18"/>
            <path d="M8 6V4h8v2"/>
            <rect x="6" y="6" width="12" height="14" rx="2"/>
            <line x1="10" y1="11" x2="10" y2="17"/>
            <line x1="14" y1="11" x2="14" y2="17"/>
          </svg>
        </button>
      `;

      li.addEventListener('click', () => {
        if (activeFolderEl) activeFolderEl.classList.remove('active');
        li.classList.add('active');
        activeFolderEl = li;
        browseDir(d);
      });

      const deleteBtn = li.querySelector('.delete-folder-btn');

      deleteBtn.addEventListener('click', async (e) => {
        e.stopPropagation();

        const confirmed = confirm(`Remove "${label}" from the index?`);
        if (!confirmed) return;

        try {
          await invoke('delete_indexed_dir', { path: d });
          setStatus(`Removed indexed folder: ${label}`);
          await loadFolders();
        } catch (err) {
          setStatus(`Delete error: ${err}`);
        }
      });

      folderList.appendChild(li);
    });
  } catch (err) {
    console.error('loadFolders error:', err);
  }
}

// ── Add folder ────────────────────────────────────────────────────────────────

btnAddFolder.addEventListener('click', async () => {
  try {
    // plugin:dialog|open returns a string path or null
    const selected = await invoke('plugin:dialog|open', { options: { directory: true, multiple: false } });
    if (!selected) return;

    const folderPath = typeof selected === 'string' ? selected : selected?.path ?? String(selected);
    if (!folderPath) return;

    setStatus(`Indexing: ${folderPath}`);
    progressCont.classList.add('visible');
    progressBar.style.width = '0%';

    await invoke('index_directory', { path: folderPath });
    await loadFolders();
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// ── Clear index ───────────────────────────────────────────────────────────────

btnClearIndex.addEventListener('click', async () => {
  if (!confirm('Clear all indexed files and tags? This cannot be undone.')) return;
  try {
    await invoke('clear_index');
    setStatus('Index cleared');
    statusCount.textContent = '';
    resultsGrid.innerHTML = '<div class="empty-state">Add a folder to start indexing, then search above.</div>';
    await loadFolders();
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// ── Open / Reveal buttons (event delegation) ──────────────────────────────────

resultsGrid.addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-action]');
  if (!btn) return;
  const { action, path } = btn.dataset;
  try {
    if (action === 'open')   await invoke('open_file',               { path });
    if (action === 'reveal') await invoke('reveal_in_file_manager',  { path });
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// ── Search input (debounced 300ms) ────────────────────────────────────────────

searchInput.addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => performSearch(e.target.value), 300);
});

// ── Navigate up ───────────────────────────────────────────────────────────────

btnUp.addEventListener('click', () => {
  if (!currentDir) return;
  const normalized = currentDir.replace(/\\/g, '/');
  const parent = normalized.replace(/\/[^/]+\/?$/, '') || normalized;
  if (parent && parent !== normalized) {
    browseDir(parent.replace(/\//g, '\\'));
  }
});

// ── Indexing progress events ──────────────────────────────────────────────────

listen('index-status', (event) => {
  const s = event.payload;

  if (s.total > 0) {
    const pct = Math.min(100, Math.round((s.indexed / s.total) * 100));
    progressCont.classList.add('visible');
    progressBar.style.width = `${pct}%`;
    setStatus(s.current_file ? `Indexing: ${s.current_file}` : 'Indexing…');
    statusCount.textContent = `${s.indexed} / ${s.total}`;
  } else {
    setStatus(s.current_file || 'Indexing…');
  }

  if (!s.is_running) {
    progressCont.classList.remove('visible');
    progressBar.style.width = '0%';
    setStatus(s.current_file || 'Ready');
    loadFolders();
  }
});

// ── Init ──────────────────────────────────────────────────────────────────────

(async function init() {
  await loadFolders();
  resultsGrid.innerHTML = '<div class="empty-state">Add a folder to start indexing, then search above.</div>';
})();
