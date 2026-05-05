const { app, BrowserWindow, ipcMain, shell, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

let mainWindow = null;
let backendProcess = null;

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
    copyDirSync(bundledAssetsDir, targetAssetsDir);
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

function copyDirSync(src, dest) {
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }
  const entries = fs.readdirSync(src, { withFileTypes: true });
  entries.forEach(entry => {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDirSync(srcPath, destPath);
    } else {
      // Only copy if dest doesn't exist or src is newer
      if (!fs.existsSync(destPath) || fs.statSync(srcPath).mtime > fs.statSync(destPath).mtime) {
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

  console.log('Starting backend:', binPath);

  backendProcess = spawn(binPath, ['serve', '--port', '8765', '--host', '127.0.0.1'], {
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

  // Wait for the backend to be ready
  const http = require('http');
  const backendUrl = 'http://127.0.0.1:8765/health';
  const maxRetries = 30;
  for (let i = 0; i < maxRetries; i++) {
    await new Promise(resolve => setTimeout(resolve, 500));
    try {
      await new Promise((resolve, reject) => {
        const req = http.get(backendUrl, (res) => {
          resolve(res.statusCode);
        });
        req.on('error', reject);
        req.setTimeout(2000, () => { req.destroy(); reject(new Error('timeout')); });
      });
      console.log('Backend is ready.');
      return true;
    } catch (e) {
      // still waiting
    }
  }
  console.warn('Backend did not become ready in time. Continuing anyway.');
  return false;
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
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    trafficLightPosition: { x: 16, y: 18 },
    autoHideMenuBar: process.platform === 'win32',
    vibrancy: process.platform === 'darwin' ? 'under-window' : undefined,
    visualEffectState: 'active',
    backgroundColor: '#0a0a0f',
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

  mainWindow.loadFile(path.join(__dirname, 'renderer', 'index.html'));

  // Open DevTools in development
  if (!app.isPackaged) {
    // mainWindow.webContents.openDevTools({ mode: 'detach' });
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
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
  return 'http://127.0.0.1:8765';
});

// ── App Lifecycle ─────────────────────────────────────────────
app.whenReady().then(async () => {
  await startBackend();
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
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

