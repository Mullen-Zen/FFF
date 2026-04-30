// clear splash screen
setTimeout(() => {
  const splash = document.getElementById('splash');
  if (splash) {
    splash.classList.add('fade-out');
    splash.addEventListener('transitionend', () => splash.remove(), { once: true });
  }
}, 3000);

const { invoke } = window.__TAURI__.core;
const { listen }  = window.__TAURI__.event;
const os = await invoke('get_os');

// geometric glyphs for file types
const G = `viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"`;

const GLYPHS = {
  folder:  `<svg ${G}><path d="M1 4h5l2 2h7v7H1V4z"/></svg>`,
  image:   `<svg ${G}><rect x="1" y="2" width="14" height="12"/><circle cx="5.5" cy="6.5" r="1.5" fill="currentColor" stroke="none"/><polyline points="1,13 5,9 8,12 11,9 15,13"/></svg>`,
  video:   `<svg ${G}><rect x="1" y="2" width="14" height="12"/><path d="M6 5.5l5 2.5-5 2.5V5.5z" fill="currentColor" stroke="none"/></svg>`,
  audio:   `<svg ${G}><line x1="2" y1="14" x2="2" y2="10"/><line x1="5" y1="14" x2="5" y2="5"/><line x1="8" y1="14" x2="8" y2="3"/><line x1="11" y1="14" x2="11" y2="6"/><line x1="14" y1="14" x2="14" y2="9"/></svg>`,
  code:    `<svg ${G}><polyline points="5,3 1,8 5,13"/><polyline points="11,3 15,8 11,13"/><line x1="9" y1="2" x2="7" y2="14"/></svg>`,
  doc:     `<svg ${G}><path d="M3 1h7l3 3v11H3V1z"/><path d="M10 1v3h3"/><line x1="5" y1="8" x2="11" y2="8"/><line x1="5" y1="11" x2="11" y2="11"/></svg>`,
  archive: `<svg ${G}><rect x="2" y="1" width="12" height="14"/><line x1="2" y1="5" x2="14" y2="5"/><line x1="6" y1="1" x2="6" y2="5"/><line x1="10" y1="1" x2="10" y2="5"/><line x1="5" y1="8" x2="11" y2="8"/></svg>`,
  data:    `<svg ${G}><rect x="1" y="1" width="14" height="14"/><line x1="1" y1="6" x2="15" y2="6"/><line x1="1" y1="11" x2="15" y2="11"/><line x1="6" y1="1" x2="6" y2="15"/></svg>`,
  exe:     `<svg ${G}><polygon points="8,1 14,4.5 14,11.5 8,15 2,11.5 2,4.5"/></svg>`,
  default: `<svg ${G}><path d="M3 1h7l3 3v11H3V1z"/><path d="M10 1v3h3"/></svg>`,
};

const EXT_MAP = {
  image:   ['jpg','jpeg','png','gif','svg','webp','bmp','ico','tiff'],
  video:   ['mp4','mkv','avi','mov','webm','flv','wmv','m4v'],
  audio:   ['mp3','wav','flac','ogg','aac','m4a','opus','wma'],
  code:    ['rs','go','py','js','ts','jsx','tsx','html','css','scss','json','toml','yaml','yml','sh','bash','bat','ps1','c','cpp','h','hpp','java','rb','kt','swift','php','lua'],
  doc:     ['pdf','doc','docx','odt','rtf','txt','md','log','ppt','pptx'],
  archive: ['zip','rar','7z','tar','gz','bz2','xz','tgz'],
  data:    ['xls','xlsx','csv','db','sqlite','sql','ods'],
  exe:     ['exe','dll','msi','app','deb','rpm'],
};

function fileGlyph(ext) {
  const e = ext?.toLowerCase();
  for (const [type, exts] of Object.entries(EXT_MAP)) {
    if (exts.includes(e)) return GLYPHS[type];
  }
  return GLYPHS.default;
}

const folderList    = document.getElementById('folder-list');
const recentsList   = document.getElementById('recents-list');
const btnAddFolder  = document.getElementById('btn-add-folder');
const btnClearIndex = document.getElementById('btn-clear-index');
const searchInput   = document.getElementById('search-input');
const resultsList   = document.getElementById('results-list');
const statusInline  = document.getElementById('status-inline');
const browseBar     = document.getElementById('browse-bar');
const currentPath   = document.getElementById('current-path');
const btnUp         = document.getElementById('btn-up');

let currentDir     = null;
let searchTimer    = null;
let activeFolderEl = null;

function formatBytes(bytes) {
  if (!bytes) return '';
  if (bytes < 1024)       return bytes + ' B';
  if (bytes < 1024 ** 2)  return (bytes / 1024).toFixed(1) + ' KB';
  if (bytes < 1024 ** 3)  return (bytes / 1024 ** 2).toFixed(1) + ' MB';
  return (bytes / 1024 ** 3).toFixed(1) + ' GB';
}

function setStatus(text) {
  statusInline.textContent = text;
}

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

function renderFileRow(f) {
  const ext = f.extension ?? f.name.split('.').pop() ?? '';
  const row = document.createElement('div');
  row.className = 'file-row';

  const tagsHtml = (f.tags ?? [])
    .map(t => `<span class="tag ai">${escHtml(t)}</span>`)
    .join('');

  row.innerHTML = `
    <div class="file-glyph">${fileGlyph(ext)}</div>
    <div class="file-info">
      <div class="file-name" title="${escAttr(f.name)}">${escHtml(f.name)}</div>
      <div class="file-meta" title="${escAttr(f.path)}">${escHtml(f.path)}</div>
      ${tagsHtml ? `<div class="file-tags">${tagsHtml}</div>` : ''}
    </div>
    <div class="row-actions">
      <button data-action="open"   data-path="${escAttr(f.path)}">Open</button>
      <button data-action="reveal" data-path="${escAttr(f.path)}">Reveal</button>
    </div>`;
  return row;
}

function renderDirRow(e) {
  const ext  = e.is_dir ? '' : (e.name.split('.').pop() ?? '');
  const row  = document.createElement('div');
  row.className = 'file-row' + (e.is_dir ? ' is-dir' : '');
  const meta = e.is_dir ? 'folder' : formatBytes(e.size);

  const tagsHtml = (e.tags ?? [])
    .map(t => `<span class="tag ai">${escHtml(t)}</span>`)
    .join('');

  const actionsHtml = e.is_dir ? '' : `
    <div class="row-actions">
      <button data-action="open"   data-path="${escAttr(e.path)}">Open</button>
      <button data-action="reveal" data-path="${escAttr(e.path)}">Reveal</button>
    </div>`;

  row.innerHTML = `
    <div class="file-glyph">${e.is_dir ? GLYPHS.folder : fileGlyph(ext)}</div>
    <div class="file-info">
      <div class="file-name" title="${escAttr(e.name)}">${escHtml(e.name)}</div>
      <div class="file-meta">${escHtml(meta)}</div>
      ${tagsHtml ? `<div class="file-tags">${tagsHtml}</div>` : ''}
    </div>
    ${actionsHtml}`;

  if (e.is_dir) row.addEventListener('dblclick', () => browseDir(e.path));
  return row;
}

// recents

const RECENTS_KEY = 'fff_recents';

function getRecents() {
  try { return JSON.parse(localStorage.getItem(RECENTS_KEY) ?? '[]'); }
  catch { return []; }
}

function addRecent(file) {
  const next = [file, ...getRecents().filter(r => r.path !== file.path)].slice(0, 8);
  localStorage.setItem(RECENTS_KEY, JSON.stringify(next));
  renderRecents();
}

function renderRecents() {
  recentsList.innerHTML = '';
  const recents = getRecents();

  if (!recents.length) {
    const li = document.createElement('li');
    li.className = 'rail-item';
    li.innerHTML = `<span class="rail-icon"></span><span class="rail-label" style="font-style:italic">None yet</span>`;
    recentsList.appendChild(li);
    return;
  }

  recents.forEach(r => {
    const li = document.createElement('li');
    li.className = 'rail-item';
    li.title = r.path;
    li.innerHTML = `<span class="rail-icon">${fileGlyph(r.ext)}</span><span class="rail-label rail-label-mono">${escHtml(r.name)}</span>`;
    li.addEventListener('click', async () => {
      try { await invoke('open_file', { path: r.path }); }
      catch (err) { setStatus(`Error: ${err}`); }
    });
    recentsList.appendChild(li);
  });
}

// browse

async function browseDir(path) {
  try {
    currentDir = path;
    currentPath.textContent = path;
    browseBar.classList.add('visible');
    searchInput.value = '';

    const entries = await invoke('browse_directory', { path });
    resultsList.innerHTML = '';

    if (!entries.length) {
      resultsList.innerHTML = '<div class="empty-state">This folder is empty.</div>';
    } else {
      entries.forEach(e => resultsList.appendChild(renderDirRow(e)));
    }
    setStatus(`${entries.length} item${entries.length !== 1 ? 's' : ''}`);
  } catch (err) {
    setStatus(`Browse error: ${err}`);
  }
}

// search

async function performSearch(query) {
  if (!query.trim()) {
    browseBar.classList.remove('visible');
    currentDir = null;
    resultsList.innerHTML = '';

    const recents = getRecents();
    if (recents.length) {
      const label = document.createElement('div');
      label.className = 'results-section-label';
      label.textContent = 'Recent';
      resultsList.appendChild(label);

      recents.forEach(r => {
        const row = document.createElement('div');
        row.className = 'file-row';
        row.innerHTML = `
          <div class="file-glyph">${fileGlyph(r.ext)}</div>
          <div class="file-info">
            <div class="file-name">${escHtml(r.name)}</div>
            <div class="file-meta">${escHtml(r.path)}</div>
          </div>
          <div class="row-actions">
            <button data-action="open"   data-path="${escAttr(r.path)}">Open</button>
            <button data-action="reveal" data-path="${escAttr(r.path)}">Reveal</button>
          </div>`;
        resultsList.appendChild(row);
      });
      setStatus('Recent files');
    } else {
      resultsList.innerHTML = '<div class="empty-state">Add a folder to start indexing, then search above.</div>';
      setStatus('');
    }
    return;
  }

  browseBar.classList.remove('visible');
  currentDir = null;

  try {
    const results = await invoke('search_files', { query });
    resultsList.innerHTML = '';

    if (!results.length) {
      resultsList.innerHTML = `<div class="empty-state">No results for "<strong>${escHtml(query)}</strong>".</div>`;
    } else {
      results.forEach(f => resultsList.appendChild(renderFileRow(f)));
    }
    setStatus(`${results.length} result${results.length !== 1 ? 's' : ''}`);
  } catch (err) {
    setStatus(`Search error: ${err}`);
  }
}

// sidebar folder list

async function loadFolders() {
  try {
    const dirs = await invoke('get_indexed_dirs');
    folderList.innerHTML = '';

    if (!dirs.length) {
      const li = document.createElement('li');
      li.className = 'rail-item';
      li.innerHTML = `<span class="rail-icon"></span><span class="rail-label" style="font-style:italic">No folders yet</span>`;
      folderList.appendChild(li);
      return;
    }

    dirs.forEach(d => {
      const li = document.createElement('li');
      li.className = 'rail-item';
      const label = d.replace(/\\/g, '/').split('/').filter(Boolean).pop() || d;
      li.title = d;
      li.innerHTML = `<span class="rail-icon">${GLYPHS.folder}</span><span class="rail-label">${escHtml(label)}</span>`;
      li.addEventListener('click', () => {
        if (activeFolderEl) activeFolderEl.classList.remove('active');
        li.classList.add('active');
        activeFolderEl = li;
        browseDir(d);
      });
      folderList.appendChild(li);
    });
  } catch (err) {
    console.error('loadFolders:', err);
  }
}

// add folder

btnAddFolder.addEventListener('click', async () => {
  try {
    const selected = await invoke('plugin:dialog|open', { options: { directory: true, multiple: false } });
    if (!selected) return;

    const folderPath = typeof selected === 'string' ? selected : selected?.path ?? String(selected);
    if (!folderPath) return;

    setStatus(`Indexing: ${folderPath}`);
    await invoke('index_directory', { path: folderPath });
    await loadFolders();
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// clear index

btnClearIndex.addEventListener('click', async () => {
  if (!confirm('Clear all indexed files and tags?')) return;
  try {
    await invoke('clear_index');
    setStatus('Index cleared');
    performSearch('');
    await loadFolders();
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// open or reveal

resultsList.addEventListener('click', async (e) => {
  const btn = e.target.closest('[data-action]');
  if (!btn) return;
  const { action, path } = btn.dataset;
  const name = btn.closest('.file-row')?.querySelector('.file-name')?.textContent ?? path.split(/[\\/]/).pop();
  const ext  = name.includes('.') ? name.split('.').pop() : '';
  try {
    if (action === 'open')   await invoke('open_file',              { path });
    if (action === 'reveal') await invoke('reveal_in_file_manager', { path });
    addRecent({ name, path, ext });
  } catch (err) {
    setStatus(`Error: ${err}`);
  }
});

// search bar

searchInput.addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => performSearch(e.target.value), 300);
});

// crawl up

btnUp.addEventListener('click', () => {
  if (!currentDir) return;
  const normalized = currentDir.replace(/\\/g, '/');
  const parent = normalized.replace(/\/[^/]+\/?$/, '') || normalized;
  if (parent && parent !== normalized) {
    const finalPath = os === 'windows' ? parent.replace(/\//g, '\\') : parent;
    browseDir(finalPath);
  }
});

// slash to search

document.addEventListener('keydown', (e) => {
  if (e.key === '/' && document.activeElement !== searchInput) {
    e.preventDefault();
    searchInput.focus();
    searchInput.select();
  }
});

// indexing progress

listen('index-status', (event) => {
  const s = event.payload;
  if (s.is_running) {
    const pct  = s.total > 0 ? ` ${Math.round(s.indexed / s.total * 100)}%` : '';
    const file = s.current_file ? ` · ${s.current_file.split(/[\\/]/).pop()}` : '';
    setStatus(`Indexing${pct}${file}`);
  } else {
    setStatus('Ready');
    loadFolders();
  }
});

// light/dark theme

const THEME_KEY  = 'fff_theme';
const MOON_SVG   = `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 9A5 5 0 1 1 7 3a4 4 0 0 0 6 6z"/></svg>`;
const SUN_SVG    = `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><circle cx="8" cy="8" r="3"/><line x1="8" y1="1" x2="8" y2="3"/><line x1="8" y1="13" x2="8" y2="15"/><line x1="1" y1="8" x2="3" y2="8"/><line x1="13" y1="8" x2="15" y2="8"/><line x1="3.2" y1="3.2" x2="4.6" y2="4.6"/><line x1="11.4" y1="11.4" x2="12.8" y2="12.8"/><line x1="3.2" y1="12.8" x2="4.6" y2="11.4"/><line x1="11.4" y1="4.6" x2="12.8" y2="3.2"/></svg>`;
const btnTheme   = document.getElementById('btn-theme');
const themeIcon  = document.getElementById('theme-icon');
const themeLabel = document.getElementById('theme-label');

function applyTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  if (theme === 'dark') {
    themeIcon.innerHTML  = SUN_SVG;
    themeLabel.textContent = 'Light Mode';
  } else {
    themeIcon.innerHTML  = MOON_SVG;
    themeLabel.textContent = 'Dark Mode';
  }
}

applyTheme(localStorage.getItem(THEME_KEY) ?? 'light');

btnTheme.addEventListener('click', () => {
  const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
  localStorage.setItem(THEME_KEY, next);
  applyTheme(next);
});

// init

(async function init() {
  await loadFolders();
  renderRecents();
  performSearch('');
})();
