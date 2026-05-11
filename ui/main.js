const { app, BrowserWindow, ipcMain, shell, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');
const net = require('net');

let mainWindow = null;
let backendProcess = null;
let startupErrorShown = false;
let launchPromise = null;
const BACKEND_HOST = '127.0.0.1';
const DEFAULT_BACKEND_PORT = 8765;
let backendPort = DEFAULT_BACKEND_PORT;

function backendOrigin() {
  return `http://${BACKEND_HOST}:${backendPort}`;
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function isBackendReady() {
  const http = require('http');
  return new Promise((resolve, reject) => {
    const req = http.get(`${backendOrigin()}/health`, (res) => {
      resolve(res.statusCode && res.statusCode >= 200 && res.statusCode < 300);
    });
    req.on('error', reject);
    req.setTimeout(2000, () => {
      req.destroy();
      reject(new Error('timeout'));
    });
  });
}

async function fetchBackendText(pathname) {
  const http = require('http');
  return new Promise((resolve, reject) => {
    const req = http.get(`${backendOrigin()}${pathname}`, (res) => {
      let body = '';
      res.setEncoding('utf8');
      res.on('data', (chunk) => {
        body += chunk;
      });
      res.on('end', () => {
        resolve({
          statusCode: res.statusCode || 0,
          body,
        });
      });
    });
    req.on('error', reject);
    req.setTimeout(3000, () => {
      req.destroy();
      reject(new Error('timeout'));
    });
  });
}

function canListenOnPort(port) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', (error) => {
      if (error.code === 'EADDRINUSE' || error.code === 'EACCES') {
        resolve(false);
        return;
      }
      reject(error);
    });
    server.once('listening', () => {
      server.close(() => resolve(true));
    });
    server.listen(port, BACKEND_HOST);
  });
}

async function findAvailablePort(startPort = DEFAULT_BACKEND_PORT) {
  for (let port = startPort; port < startPort + 100; port++) {
    if (await canListenOnPort(port)) {
      return port;
    }
  }

  throw new Error(`No available localhost port found from ${startPort} to ${startPort + 99}`);
}

function getErrorPageUrl(title, detail) {
  const html = `<!DOCTYPE html>
  <html lang="en">
    <head>
      <meta charset="UTF-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1.0" />
      <title>${escapeHtml(title)}</title>
      <style>
        :root { color-scheme: dark; }
        body {
          margin: 0;
          min-height: 100vh;
          display: grid;
          place-items: center;
          background: #1e1e1e;
          color: #f8fafc;
          font: 14px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }
        main {
          width: min(760px, calc(100vw - 32px));
          padding: 24px;
          border-radius: 16px;
          border: 1px solid #ef4444;
          background: #2a1111;
          box-shadow: 0 20px 64px rgba(0, 0, 0, 0.45);
        }
        h1 { margin: 0 0 12px; color: #fecaca; font-size: 20px; }
        pre {
          margin: 12px 0 0;
          padding: 12px;
          overflow: auto;
          border-radius: 8px;
          background: #111827;
          color: #bfdbfe;
          white-space: pre-wrap;
        }
      </style>
    </head>
    <body>
      <main>
        <h1>${escapeHtml(title)}</h1>
        <p>Generic Coder could not render the workbench, so this diagnostic page is shown instead of a black screen.</p>
        <pre>${escapeHtml(detail)}</pre>
      </main>
    </body>
  </html>`;

  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

async function showStartupError(title, detail) {
  if (!mainWindow || mainWindow.isDestroyed()) {
    return;
  }

  startupErrorShown = true;
  console.error(`${title}: ${detail}`);
  await mainWindow.loadURL(getErrorPageUrl(title, detail));
  mainWindow.show();
}

async function loadWorkbenchWhenReady(options = {}) {
  const {
    retries = 60,
    delayMs = 500,
  } = options;

  for (let i = 0; i < retries; i++) {
    if (!mainWindow || mainWindow.isDestroyed()) {
      return false;
    }

    try {
      const ready = await isBackendReady();
      if (ready) {
        const response = await fetchBackendText('/');
        if (response.statusCode < 200 || response.statusCode >= 300 || !response.body.trim()) {
          await showStartupError(
            'Generic Coder frontend is unavailable',
            `GET / returned HTTP ${response.statusCode} with ${response.body.length} bytes.`,
          );
          return false;
        }
        if (!response.body.includes('/static/app.js')) {
          await showStartupError(
            'Generic Coder frontend assets are missing',
            `The backend is healthy, but the workbench HTML does not reference /static/app.js.\n\nResponse preview:\n${response.body.slice(0, 1200)}`,
          );
          return false;
        }
        await mainWindow.loadURL(backendOrigin());
        mainWindow.show();
        return true;
      }
    } catch (error) {
      // keep polling from the main process; renderer-side fetches from data: URLs are unreliable
    }

    await wait(delayMs);
  }

  return false;
}

function getBackendPath() {
  const isDev = !app.isPackaged;
  const platform = process.platform;
  const binaryName = platform === 'win32' ? 'generic-coder-backend.exe' : 'generic-coder-backend';

  if (isDev) {
    // In development, look for the Rust binary in the workspace target dir
    const workspaceTarget = path.join(__dirname, '..', 'target', 'release', 'generic-coder');
    if (fs.existsSync(workspaceTarget)) return workspaceTarget;
    // Fallback: look for the staged binary
    const stagedTarget = path.join(__dirname, 'bin', binaryName);
    if (fs.existsSync(stagedTarget)) return stagedTarget;
    return workspaceTarget; // return default even if missing
  }

  // In production, use the bundled binary from extraResources
  return path.join(process.resourcesPath, 'bin', binaryName);
}

function getProjectDataDir() {
  const isDev = !app.isPackaged;

  if (isDev) {
    // In dev, use the workspace directory
    return path.join(__dirname, '..');
  }

  // In production, use ~/Library/Application Support on macOS,
  // %APPDATA% on Windows, ~/.local/share on Linux
  const appData = process.platform === 'darwin'
    ? path.join(app.getPath('home'), 'Library', 'Application Support', 'Generic Coder')
    : process.platform === 'win32'
      ? path.join(app.getPath('appData'), 'Generic Coder')
      : path.join(app.getPath('home'), '.local', 'share', 'generic-coder');

  return appData;
}

function ensureProjectDir(projectDir) {
  // Create the full directory tree that the backend needs
  const dirs = [
    projectDir,
    path.join(projectDir, 'assets'),
    path.join(projectDir, 'memory'),
    path.join(projectDir, 'memory', 'errors'),
    path.join(projectDir, 'skills'),
    path.join(projectDir, 'temp'),
  ];

  dirs.forEach(dir => {
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
      console.log('  created:', dir);
    }
  });

  // Copy assets from the app bundle into the writable project dir
  const bundledAssetsDir = path.join(__dirname, 'assets');
  const targetAssetsDir = path.join(projectDir, 'assets');
  if (fs.existsSync(bundledAssetsDir)) {
    const bundledFrontendDir = path.join(bundledAssetsDir, 'generic_coder');
    const targetFrontendDir = path.join(targetAssetsDir, 'generic_coder');
    syncBundledFrontend(bundledFrontendDir, targetFrontendDir);
    copyDirSync(bundledAssetsDir, targetAssetsDir, { overwrite: true });
    console.log('  assets synced');
  } else {
    console.log('  no bundled assets found at:', bundledAssetsDir);
  }

  // Copy bundled skills into the writable skills dir
  const bundledSkillsDir = path.join(bundledAssetsDir, 'skills');
  const targetSkillsDir = path.join(projectDir, 'skills');
  if (fs.existsSync(bundledSkillsDir)) {
    copyDirSync(bundledSkillsDir, targetSkillsDir);
    console.log('  skills synced');
  }
}

function syncBundledFrontend(src, dest) {
  if (!fs.existsSync(src)) {
    return;
  }

  fs.rmSync(dest, { recursive: true, force: true });
  copyDirSync(src, dest, { overwrite: true });
}

function copyDirSync(src, dest, options = {}) {
  const { overwrite = false } = options;
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }
  const entries = fs.readdirSync(src, { withFileTypes: true });
  entries.forEach(entry => {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDirSync(srcPath, destPath, options);
    } else {
      if (
        overwrite ||
        !fs.existsSync(destPath) ||
        fs.statSync(srcPath).mtime > fs.statSync(destPath).mtime
      ) {
        fs.copyFileSync(srcPath, destPath);
      }
    }
  });
}

async function startBackend() {
  const binPath = getBackendPath();

  if (!fs.existsSync(binPath)) {
    console.warn('Backend binary not found at:', binPath);
    console.warn('Please start the Generic Coder server manually.');
    return false;
  }

  // Set up the writable project directory
  const projectDir = getProjectDataDir();
  console.log('Project data directory:', projectDir);
  ensureProjectDir(projectDir);

  backendPort = await findAvailablePort();
  console.log('Starting backend:', binPath);
  console.log('Backend URL:', backendOrigin());

  backendProcess = spawn(binPath, ['serve', '--port', String(backendPort), '--host', BACKEND_HOST], {
    cwd: projectDir,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: {
      ...process.env,
      GENERIC_CODER_PROJECT_DIR: projectDir,
    }
  });

  backendProcess.stdout.on('data', (data) => {
    console.log('[backend]', data.toString().trim());
  });

  backendProcess.stderr.on('data', (data) => {
    console.error('[backend]', data.toString().trim());
  });

  backendProcess.on('error', (err) => {
    console.error('Backend process error:', err.message);
  });

  backendProcess.on('exit', (code) => {
    console.log('Backend exited with code:', code);
    backendProcess = null;
  });

  const maxRetries = 30;
  for (let i = 0; i < maxRetries; i++) {
    await wait(500);
    try {
      const ready = await isBackendReady();
      if (ready) {
        console.log('Backend is ready.');
        return true;
      }
    } catch (e) {
      // still waiting
    }
  }
  console.warn('Backend did not become ready in time. Continuing anyway.');
  return false;
}

async function ensureBackendRunning() {
  if (!backendProcess) {
    return startBackend();
  }

  try {
    const ready = await isBackendReady();
    if (ready) {
      console.log('Reusing existing backend:', backendOrigin());
      return true;
    }
  } catch (error) {
    // restart below
  }

  console.warn('Existing backend is unavailable. Restarting it...');
  stopBackend();
  await wait(200);
  return startBackend();
}

function stopBackend() {
  if (backendProcess) {
    console.log('Stopping backend...');
    backendProcess.kill('SIGTERM');
    backendProcess = null;
  }
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 960,
    minWidth: 1000,
    minHeight: 640,
    title: 'Generic Coder',
    show: false,
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    trafficLightPosition: { x: 16, y: 18 },
    autoHideMenuBar: process.platform === 'win32',
    backgroundColor: '#1e1e1e',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  if (process.platform === 'win32') {
    mainWindow.setMenuBarVisibility(false);
  }

  // Show window only when the renderer is ready to avoid a black flash
  mainWindow.once('ready-to-show', () => {
    mainWindow.show();
  });

  setTimeout(() => {
    if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isVisible()) {
      mainWindow.show();
    }
  }, 3000);

  mainWindow.webContents.on('did-fail-load', (_event, errorCode, errorDescription, validatedURL) => {
    if (validatedURL.startsWith('data:')) {
      return;
    }
    void showStartupError(
      'Generic Coder page failed to load',
      `${validatedURL}\n${errorCode}: ${errorDescription}`,
    );
  });

  mainWindow.webContents.on('render-process-gone', (_event, details) => {
    void showStartupError(
      'Generic Coder renderer process crashed',
      JSON.stringify(details, null, 2),
    );
  });

  mainWindow.on('unresponsive', () => {
    void showStartupError(
      'Generic Coder window became unresponsive',
      'The Electron renderer stopped responding during startup.',
    );
  });

  mainWindow.loadURL(getLoadingPageUrl());

  // Open DevTools in development for debugging
  if (!app.isPackaged) {
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

async function launchWorkbench() {
  if (launchPromise) {
    return launchPromise;
  }

  launchPromise = (async () => {
    startupErrorShown = false;

    if (!mainWindow || mainWindow.isDestroyed()) {
      createWindow();
    } else {
      await mainWindow.loadURL(getLoadingPageUrl());
    }

    let backendReady = false;
    try {
      backendReady = await ensureBackendRunning();
    } catch (error) {
      await showStartupError(
        'Generic Coder backend failed to start',
        error && error.stack ? error.stack : String(error),
      );
    }

    const loaded = startupErrorShown
      ? false
      : await loadWorkbenchWhenReady({ retries: backendReady ? 3 : 60, delayMs: 500 });

    if (!loaded && !startupErrorShown && mainWindow && !mainWindow.isDestroyed()) {
      await showStartupError(
        'Generic Coder failed to start',
        'The backend did not become ready, so the shared workbench could not be loaded. Check the app logs for backend startup errors.',
      );
    }
  })().finally(() => {
    launchPromise = null;
  });

  return launchPromise;
}

function getLoadingPageUrl() {
  const html = `<!DOCTYPE html>
  <html lang="en">
    <head>
      <meta charset="UTF-8" />
      <meta name="viewport" content="width=device-width, initial-scale=1.0" />
      <title>Generic Coder</title>
      <style>
        :root { color-scheme: dark; }
        body {
          margin: 0;
          min-height: 100vh;
          display: grid;
          place-items: center;
          background: #111827;
          color: #f8fafc;
          font: 14px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }
        .card {
          width: min(420px, calc(100vw - 32px));
          padding: 24px;
          border-radius: 18px;
          border: 1px solid rgba(148, 163, 184, 0.18);
          background: rgba(15, 23, 42, 0.88);
          box-shadow: 0 20px 60px rgba(0, 0, 0, 0.35);
        }
        .spinner {
          width: 28px;
          height: 28px;
          border-radius: 999px;
          border: 3px solid rgba(255, 255, 255, 0.18);
          border-top-color: #38bdf8;
          animation: spin 0.9s linear infinite;
          margin-bottom: 14px;
        }
        h1 { margin: 0 0 8px; font-size: 18px; }
        p { margin: 0; color: #cbd5e1; }
        @keyframes spin { to { transform: rotate(360deg); } }
      </style>
    </head>
    <body>
      <div class="card">
        <div class="spinner"></div>
        <h1>Launching Generic Coder</h1>
        <p>The desktop shell is starting the local Rust backend and loading the shared workbench.</p>
      </div>
    </body>
  </html>`;

  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

// ── IPC Handlers ──────────────────────────────────────────────
ipcMain.handle('open-external', async (_event, url) => {
  await shell.openExternal(url);
});

ipcMain.handle('get-platform', () => process.platform);

ipcMain.handle('pick-folder', async () => {
  if (!mainWindow) return null;
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openDirectory']
  });
  return result.canceled ? null : result.filePaths[0];
});

ipcMain.handle('get-backend-url', () => {
  return backendOrigin();
});

// ── App Lifecycle ─────────────────────────────────────────────
app.whenReady().then(async () => {
  await launchWorkbench();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      void launchWorkbench();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('before-quit', () => {
  stopBackend();
});
