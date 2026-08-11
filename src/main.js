import { open } from '@tauri-apps/plugin-dialog';

const { invoke } = window.__TAURI__.core;

// Global State Management
let leftIndex = 0;
let rightIndex = 0;
let currentWorkspacePath = "";

const leftTitles = ["L1: GitHub & Scripting", "L2: Theory Development", "L3: Empirical testing & GR"];
const rightTitles = ["R1: Speculation & Horizons", "R2: Layman Metaphor & Visuals", "R3: Wild Expansion"];

// --- Theme Handling Engine ---

function applyTheme(themeName) {
  const root = document.documentElement;
  root.setAttribute('data-theme', themeName);
  
  // Sync dropdown UI selector state if it is present in the DOM
  const themeSelector = document.getElementById('settings-theme');
  if (themeSelector) {
    themeSelector.value = themeName;
  }
  
  // Persist preference to LocalStorage
  localStorage.setItem('user-theme', themeName);
}

function initializeTheme() {
  const savedTheme = localStorage.getItem('user-theme');
  if (savedTheme) {
    applyTheme(savedTheme);
  } else {
    // Check operating system default color scheme preference
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    applyTheme(prefersDark ? 'dark' : 'light');
  }
}

// --- LLM Chat Subsystem ---

async function handleChatSend(side) {
  appendTerminalLog(`Chat send triggered for side: ${side}`);
  
  const isLeft = side === 'left';
  const cylinder = document.getElementById(isLeft ? 'left-cylinder' : 'right-cylinder');
  
  if (!cylinder) {
    appendTerminalLog(`Error: Could not find cylinder element: #${isLeft ? 'left-cylinder' : 'right-cylinder'}`);
    return;
  }

  // Robust selector fallback to find your input field
  const input = cylinder.querySelector('.chat-input') || cylinder.querySelector('input') || cylinder.querySelector('textarea');
  if (!input) {
    appendTerminalLog(`Error: Could not locate text input element inside #${side}-cylinder`);
    return;
  }

  const text = input.value.trim();
  if (!text) {
    appendTerminalLog(`Warning: Input text field is empty.`);
    return;
  }
  
  // Track rotation state
  const index = isLeft ? leftIndex : rightIndex;
  const stripId = isLeft ? 'left-strip' : 'right-strip';
  const strip = document.getElementById(stripId);
  
  // Resilient slot targeting: fall back to direct child index if class mismatch occurs
  let activeSlot = null;
  if (strip) {
    activeSlot = strip.children[index] || strip.querySelector('.stack-slot');
  }
  
  // Absolute fallback target if the structural panels can't be resolved
  if (!activeSlot) {
    activeSlot = cylinder.querySelector('.chat-history') || cylinder;
    appendTerminalLog(`Warning: Structural slot target missing. Falling back to parent container layout.`);
  }
  
  // 1. Render User Message immediately
  appendMessageBubble(activeSlot, 'Gregory', text);
  input.value = ''; // Flush input field
  
  // 2. Dispatch payload down the Tauri bridge to Rust
  try {
    appendTerminalLog(`Invoking 'send_llm_prompt' [Side: ${side}, Slot: ${index}]`);
    
    // Updated to match Rust function send_llm_prompt requirements
    const response = await invoke('send_llm_prompt', { 
      pane: side, 
      history: [{ role: "user", content: text }] 
    });
    
    // 3. Render LLM Response
    appendMessageBubble(activeSlot, 'AI Response', response);
  } catch (err) {
    appendTerminalLog(`LLM Invoke Failure: ${err}`);
    appendMessageBubble(activeSlot, 'System Error', `Backend pipeline failed: ${err}`);
  }
}

function appendMessageBubble(slot, sender, text) {
  const bubble = document.createElement('div');
  bubble.className = 'chat-bubble';
  bubble.style.margin = '6px 0';
  bubble.style.padding = '6px 10px';
  bubble.style.borderRadius = '4px';
  bubble.style.background = 'rgba(255, 255, 255, 0.03)';
  
  if (sender === 'Gregory') {
    bubble.style.borderLeft = '3px solid var(--accent, #b4befe)';
  } else if (sender.includes('Error') || sender === 'System') {
    bubble.style.borderLeft = '3px solid #f38ba8'; 
  } else {
    bubble.style.borderLeft = '3px solid #a6e3a1';
  }
  
  const escapedText = String(text).replace(/[&<>"']/g, m => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
  })[m]);

  bubble.innerHTML = `<strong>${sender}:</strong> ${escapedText}`;
  slot.appendChild(bubble);
  
  slot.scrollTop = slot.scrollHeight;
}

// --- Workspace & File System Tree Logic ---

async function loadWorkspace() {
  try {
    const selectedPath = await open({
      directory: true,
      multiple: false,
      title: "Select Workspace Directory"
    });

    if (!selectedPath) return;
    
    currentWorkspacePath = selectedPath;
    await invoke('save_root_directory', { path: selectedPath });
    
    console.log("Attempting to fetch directory for:", selectedPath); // Check this in DevTools[cite: 2]
    await fetchAndRenderDirectory(selectedPath);
  } catch (err) {
    console.error("Workspace Load Error:", err); // Check this in DevTools[cite: 2]
    appendTerminalLog(`Error loading workspace: ${err}`);
  }
}

async function fetchAndRenderDirectory(path, containerElement = null) {
  // GUARD: If this specific container has already been loaded, do nothing
  if (containerElement && containerElement.dataset.loaded === "true") {
    return;
  }

  try {
    console.log("Fetching directory contents for:", path);
    const container = containerElement || document.getElementById('file-tree-container');
    
    if (!containerElement) {
      container.innerHTML = '';
    }

    // 1. Fetch entries from backend
    const entries = await invoke('read_directory', { path });
    console.log("Rust returned these entries:", entries);

    // Mark as loaded before rendering to prevent re-triggering during async operations
    if (containerElement) {
      containerElement.dataset.loaded = "true";
    }

    if (entries.length === 0) {
      const emptyDiv = document.createElement('div');
      emptyDiv.style.color = '#6c7086';
      emptyDiv.style.padding = '4px 12px';
      emptyDiv.style.fontStyle = 'italic';
      emptyDiv.innerText = 'Empty directory.';
      container.appendChild(emptyDiv);
      return;
    }

    entries.sort((a, b) => {
      if (a.is_dir && !b.is_dir) return -1;
      if (!a.is_dir && b.is_dir) return 1;
      return a.name.localeCompare(b.name);
    });

    const ul = document.createElement('ul');
    ul.style.listStyle = 'none';
    ul.style.paddingLeft = containerElement ? '12px' : '2px';
    ul.style.margin = '0';

    entries.forEach(entry => {
      const li = document.createElement('li');
      li.style.margin = '4px 0';

      const label = document.createElement('div');
      label.className = 'file-item';
      label.dataset.path = entry.path;
      label.dataset.isDir = String(Boolean(entry.is_dir));
      label.innerText = (entry.is_dir ? '📁 ' : '📄 ') + entry.name;
      li.appendChild(label);

      if (entry.is_dir) {
        const subContainer = document.createElement('div');
        subContainer.style.display = 'none';
        subContainer.dataset.directoryPath = entry.path;
        li.appendChild(subContainer);

        label.onclick = async (e) => {
          e.stopPropagation();
          const isCollapsed = subContainer.style.display === 'none';
          
          if (isCollapsed) {
            await fetchAndRenderDirectory(entry.path, subContainer);
            subContainer.style.display = 'block';
            subContainer.dataset.expanded = 'true';
            label.innerText = '📂 ' + entry.name;
          } else {
            subContainer.style.display = 'none';
            subContainer.dataset.expanded = 'false';
            label.innerText = '📁 ' + entry.name;
          }
        };
      } else {
        label.onclick = (e) => {
          e.stopPropagation();
          openEditor(entry.path);
        };
      }
      ul.appendChild(li);
    });

    container.appendChild(ul);
    if (!containerElement) {
      appendTerminalLog(`Loaded directory tree: ${path}`);
    }
  } catch (err) {
    console.error("Tree Read Error:", err);
    appendTerminalLog(`Tree Read Error: ${err}`);
  }
}

function buildVisibleProjectTreeMarkdown() {
  const rootPath = (currentWorkspacePath || '').trim();
  const rootContainer = document.getElementById('file-tree-container');

  if (!rootPath || !rootContainer) {
    return '';
  }

  const rootFolderName = rootPath.split(/[\\/]/).filter(Boolean).pop() || 'project';
  const normalizedRootPath = rootPath.replace(/\\/g, '/');
  const lines = [
    '# Visible Project Tree',
    '',
    '## Root',
    `- ${rootFolderName}`,
    '',
    '## Visible Entries'
  ];

  const walkTree = (container, depth) => {
    if (!container) return;

    const listItems = Array.from(container.querySelectorAll(':scope > ul > li'));
    listItems.forEach((li) => {
      const label = li.querySelector(':scope > .file-item');
      if (!label) return;

      const isDir = label.dataset.isDir === 'true';
      const entryPath = (label.dataset.path || '').replace(/\\/g, '/');
      const entryName = (label.textContent || '').replace(/^[📁📂📄]\s*/, '');
      const relativePath = entryPath.startsWith(normalizedRootPath)
        ? entryPath.slice(normalizedRootPath.length).replace(/^\/+/, '')
        : entryName;
      const prefix = '  '.repeat(depth);
      const suffix = isDir ? '/' : '';

      if (relativePath || entryName) {
        lines.push(`${prefix}- /${relativePath || entryName}${suffix}`);
      }

      const subContainer = li.querySelector(':scope > div:not(.file-item)');
      if (isDir && subContainer && subContainer.style.display !== 'none') {
        walkTree(subContainer, depth + 1);
      }
    });
  };

  walkTree(rootContainer, 0);

  if (lines.length <= 6) {
    return `${lines.join('\n')}\n- /${rootFolderName}/`;
  }

  return lines.join('\n');
}

function syncIdeaPadContext() {
  const scratchpadEl = document.getElementById('scratchpad-content');
  const scratchpadText = scratchpadEl?.innerText?.trim() || '';
  const includeTree = document.getElementById('idea-pad-include-file-tree')?.checked !== false;
  const visibleTree = includeTree ? buildVisibleProjectTreeMarkdown() : '';

  const promptParts = [
    'Idea pad sync for the current session.',
    scratchpadText || 'No current idea pad note content yet. The user is asking for guidance or planning without a written note.',
    visibleTree ? 'Visible project tree context for the AI:' : '',
    visibleTree
  ].filter(Boolean);

  const promptText = promptParts.join('\n\n');

  injectSystemPrimerMessage('left', leftIndex, 'Idea Pad Sync', promptText);
  injectSystemPrimerMessage('right', rightIndex, 'Idea Pad Sync', promptText);
  document.getElementById('briefing-indicator').innerText = '🤖 Idea Pad Context Synced';
  appendTerminalLog('Idea pad context synced to both AI threads.');
}

async function exportWorkspaceTree() {
  if (!currentWorkspacePath) {
    alert("Please load a workspace directory first!");
    return;
  }
  try {
    const msg = await invoke('export_workspace_tree', { rootPath: currentWorkspacePath });
    appendTerminalLog(msg);
    alert(msg);
  } catch (err) {
    appendTerminalLog(`Export Error: ${err}`);
  }
}

async function saveAsVersion(tag) {
  if (!currentWorkspacePath) {
    alert("Please load a workspace directory first!");
    return;
  }
  try {
    const msg = await invoke('save_as_version', { 
      tag: tag, 
      root_path: currentWorkspacePath 
    });
    appendTerminalLog(msg);
    alert(msg);
  } catch (err) {
    appendTerminalLog(`Version Save Error: ${err}`);
  }
}

async function saveEquation() {
  const equationInput = document.getElementById('latex-input');
  if (!equationInput) {
    appendTerminalLog("Save Error: Could not find #latex-input element.");
    return;
  }

  const equationContent = equationInput.value.trim();
  if (!equationContent) {
    alert("No equation content to save!");
    return;
  }

  try {
    const filePath = await window.__TAURI__.dialog.save({
      filters: [{ name: 'Markdown', extensions: ['md', 'txt'] }]
    });

    if (!filePath) return; 

    const msg = await invoke('save_equation_to_md', {
      content: equationContent,
      path: filePath 
    });

    appendTerminalLog(msg);
    alert(msg);
  } catch (err) {
    appendTerminalLog(`Save Error: ${err}`);
  }
}

// --- Cylinder Core Rotations ---

function rotateLeft(direction) {
  leftIndex = Math.max(0, Math.min(2, leftIndex + direction));
  document.getElementById('left-strip').style.transform = `translateY(-${leftIndex * 33.333}%)`;
  document.getElementById('left-title').innerText = leftTitles[leftIndex];
}

function rotateRight(direction) {
  rightIndex = Math.max(0, Math.min(2, rightIndex + direction));
  document.getElementById('right-strip').style.transform = `translateY(-${rightIndex * 33.333}%)`;
  document.getElementById('right-title').innerText = rightTitles[rightIndex];
}

// --- System Configuration & Modal Controls ---

function openSettingsModal() {
  document.getElementById('settings-modal').style.display = 'flex';
}

function closeSettingsModal() {
  document.getElementById('settings-modal').style.display = 'none';
  const saveBtn = document.getElementById("save-prefs-btn");
  const saveBtnText = document.getElementById("save-btn-text");
  saveBtn.classList.remove("success");
  saveBtn.disabled = false;
  saveBtnText.textContent = "Save Configuration";
}

// Tab switcher logic
function switchSettingsTab(tabId) {
  // Toggle displaying tab panel targets
  const panels = document.querySelectorAll('.settings-tab-panel');
  panels.forEach(p => p.style.display = 'none');
  
  const targetPanel = document.getElementById(`tab-${tabId}`);
  if (targetPanel) {
    targetPanel.style.display = 'block';
  }
  
  // Manage button styling classes
  const buttons = document.querySelectorAll('.tab-navigation .tab-btn');
  buttons.forEach(b => {
    b.classList.remove('active');
    b.style.color = '#6c7086'; // Reset color
  });
  
  const clickedBtn = Array.from(buttons).find(b => b.getAttribute('onclick').includes(tabId));
  if (clickedBtn) {
    clickedBtn.classList.add('active');
    clickedBtn.style.color = 'var(--accent)';
  }
}

// Dialogue Directory Browser Helper
async function browseForDirectory(targetInputId) {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose Folder Path"
    });
    if (selected) {
      document.getElementById(`settings-${targetInputId}`).value = selected;
    }
  } catch (err) {
    appendTerminalLog(`Directory Browse Dialog Error: ${err}`);
  }
}

// Dialogue File Browser Helper
async function browseForFile(targetInputId) {
  try {
    const selected = await open({
      directory: false,
      multiple: false,
      filters: [{ name: 'Markdown Files', extensions: ['md'] }],
      title: "Choose File Path"
    });
    if (selected) {
      document.getElementById(`settings-${targetInputId}`).value = selected;
    }
  } catch (err) {
    appendTerminalLog(`File Browse Dialog Error: ${err}`);
  }
}

async function saveSettings() {
  const saveBtn = document.getElementById("save-prefs-btn");
  const saveBtnText = document.getElementById("save-btn-text");
  
  saveBtn.disabled = true;
  saveBtnText.textContent = "Saving...";

  const editor = document.getElementById('settings-editor').value;
  const term = document.getElementById('settings-terminal').value;
  const geminiKey = document.getElementById('settings-gemini-key').value;
  const openaiKey = document.getElementById('settings-openai-key').value;
  const ollamaUrl = document.getElementById('settings-ollama-url').value;
  const leftProvider = document.getElementById('settings-left-provider').value;
  const leftModel = document.getElementById('settings-left-model').value;
  const rightProvider = document.getElementById('settings-right-provider').value;
  const rightModel = document.getElementById('settings-right-model').value;
  const projectRoot = document.getElementById('settings-project-root').value;
  const theoryDir = document.getElementById('settings-theory-dir').value;
  const masterAxiom = document.getElementById('settings-master-axiom').value;
  const themeValue = document.getElementById('settings-theme').value;
  const customAccent = document.getElementById('picker-accent').value;
  const customBgPanel = document.getElementById('picker-bg-panel').value;
  
  applyTheme(themeValue);
    
  try {
    const payload = {
      editor,
      terminal_app: term,
      gemini_key: geminiKey,
      openai_key: openaiKey,
      ollama_url: ollamaUrl,
      left_provider: leftProvider,
      left_model: leftModel,
      right_provider: rightProvider,
      right_model: rightModel,
      project_root_dir: projectRoot,
      theory_md_dir: theoryDir,
      master_axiom_file: masterAxiom,
      theme: themeValue,
      custom_accent: customAccent,
      custom_bg_panel: customBgPanel
    };

    await invoke('save_user_settings', { payload });
    
    document.getElementById('terminal-label').innerText = `${term} (~/projects/physics-ide)`;
    
    saveBtn.classList.add("success");
    saveBtnText.innerHTML = "✓ Preferences Updated";
    appendTerminalLog(`Saved configuration. Native Terminal: ${term} | Editor: ${editor}`);

    setTimeout(() => {
      closeSettingsModal();
    }, 850);
  } catch (err) {
    console.error(err);
    saveBtnText.textContent = "Save Failed";
    saveBtn.disabled = false;
  }
}

// --- Native Action Bridges ---

function openEditor(filePath) {
  if (!filePath) {
    appendTerminalLog('Editor Error: no file path provided');
    return;
  }

  const payload = {
    file_path: filePath,
    terminal_app: currentConfig?.terminal_app || 'gnome-terminal',
    editor: currentConfig?.editor || ''
  };

  invoke('launch_file_editor', payload)
    .then(msg => appendTerminalLog(msg))
    .catch(err => appendTerminalLog(`Editor Error: ${err}`));
}

function spawnDetachedTerminal() {
  invoke('detach_terminal_shell')
    .then(msg => appendTerminalLog(msg))
    .catch(err => appendTerminalLog(`Terminal Error: ${err}`));
}

// --- UI Utilities ---

function insertClipboard(math) {
  alert("Copied math string to scratchpad: " + math);
}

function appendTerminalLog(line) {
  const body = document.getElementById('terminal-output');
  if (body) {
    body.innerHTML += `<br>$ ${line}`;
    body.scrollTop = body.scrollHeight;
  }
}

function renderMathBlock() {
  const eqTarget = document.getElementById('l2-equation');
  if (eqTarget && typeof katex !== 'undefined') {
    katex.render("T_{baryon} = \\oint_{\\Sigma} P(e) \\cdot d\\vec{s} \\approx 1.008 \\text{ AMU}", eqTarget, {
      throwOnError: false,
      displayMode: true
    });
  }
}

// --- Expose Functions Globally For Inline HTML Attributes ---
window.handleChatSend = handleChatSend;
window.loadWorkspace = loadWorkspace;
window.exportWorkspaceTree = exportWorkspaceTree;
window.saveAsVersion = saveAsVersion;
window.rotateLeft = rotateLeft;
window.rotateRight = rotateRight;
window.openSettingsModal = openSettingsModal;
window.closeSettingsModal = closeSettingsModal;
window.switchSettingsTab = switchSettingsTab;
window.browseForDirectory = browseForDirectory;
window.browseForFile = browseForFile;
window.applyTheme = applyTheme;
window.saveSettings = saveSettings;
window.spawnDetachedTerminal = spawnDetachedTerminal;
window.insertClipboard = insertClipboard;
window.saveEquationToMarkdown = saveEquationToMarkdown;
window.saveEquation = saveEquation;

// --- DOM Initialization & Event Wiring ---
window.addEventListener("DOMContentLoaded", () => {
  // Initialize user theme configurations immediately
  initializeTheme();

  if (typeof katex !== 'undefined') {
    renderMathBlock();
  } else {
    window.addEventListener('load', renderMathBlock);
  }

  // Bind Left Cylinder Event Listeners
  const leftCylinder = document.getElementById('left-cylinder');
  if (leftCylinder) {
    const btn = leftCylinder.querySelector('button');
    const input = leftCylinder.querySelector('input') || leftCylinder.querySelector('textarea');
    if (btn) btn.onclick = () => handleChatSend('left');
    if (input) {
      input.onkeydown = (e) => { if (e.key === 'Enter') handleChatSend('left'); };
    }
  } else {
    console.error("Initialization error: Left cylinder DOM target missing.");
  }
  
  // Bind Right Cylinder Event Listeners
  const rightCylinder = document.getElementById('right-cylinder');
  if (rightCylinder) {
    const btn = rightCylinder.querySelector('button');
    const input = rightCylinder.querySelector('input') || rightCylinder.querySelector('textarea');
    if (btn) btn.onclick = () => handleChatSend('right');
    if (input) {
      input.onkeydown = (e) => { if (e.key === 'Enter') handleChatSend('right'); };
    }
  } else {
    console.error("Initialization error: Right cylinder DOM target missing.");
  }

  // Panel Resize Controllers
  const leftWing = document.getElementById("left-wing");
  const rightWing = document.getElementById("right-wing");
  const leftResizer = document.getElementById("left-resizer");
  const rightResizer = document.getElementById("right-resizer");

  function initResize(resizer, targetPanel, isLeftPanel) {
    if (!resizer || !targetPanel) return;
    let startX, startWidth;

    const startDrag = (e) => {
      startX = e.clientX;
      startWidth = parseInt(document.defaultView.getComputedStyle(targetPanel).width, 10);
      resizer.classList.add("dragging");
      
      document.documentElement.addEventListener("mousemove", doDrag);
      document.documentElement.addEventListener("mouseup", stopDrag);
    };

    const doDrag = (e) => {
      let newWidth = isLeftPanel ? startWidth + (e.clientX - startX) : startWidth - (e.clientX - startX);
      const minWidth = isLeftPanel ? 150 : 200;
      const maxWidth = isLeftPanel ? 450 : 500;
      
      if (newWidth >= minWidth && newWidth <= maxWidth) {
        targetPanel.style.width = `${newWidth}px`;
      }
    };

    const stopDrag = () => {
      resizer.classList.remove("dragging");
      document.documentElement.removeEventListener("mousemove", doDrag);
      document.documentElement.removeEventListener("mouseup", stopDrag);
    };

    resizer.addEventListener("mousedown", startDrag);
  }

  initResize(leftResizer, leftWing, true);
  initResize(rightResizer, rightWing, false);
});
