import { build } from 'esbuild';
import { cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const uiRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(uiRoot, '..');
const workbenchRoot = path.join(uiRoot, 'workbench');
const outputRoot = path.join(repoRoot, 'assets', 'generic_coder');
const bundledOutputRoot = path.join(uiRoot, 'assets', 'generic_coder');
const vendorRoot = path.join(outputRoot, 'vendor');

async function ensureDir(dir) {
  await mkdir(dir, { recursive: true });
}

async function copyVendorAssets() {
  const monacoSource = path.join(uiRoot, 'node_modules', 'monaco-editor', 'min', 'vs');
  const monacoTarget = path.join(vendorRoot, 'monaco', 'vs');
  const codiconSource = path.join(uiRoot, 'node_modules', '@vscode', 'codicons', 'dist');
  const codiconTarget = path.join(vendorRoot, 'codicons');

  await rm(monacoTarget, { recursive: true, force: true });
  await rm(codiconTarget, { recursive: true, force: true });
  await ensureDir(monacoTarget);
  await ensureDir(codiconTarget);

  await cp(monacoSource, monacoTarget, { recursive: true });
  await cp(path.join(codiconSource, 'codicon.css'), path.join(codiconTarget, 'codicon.css'));
  await cp(path.join(codiconSource, 'codicon.ttf'), path.join(codiconTarget, 'codicon.ttf'));

  const assetNames = await readdir(path.join(uiRoot, 'node_modules', 'monaco-editor', 'min', 'vs', 'assets'));
  const pick = (prefix) => assetNames.find((name) => name.startsWith(prefix));
  const generatedSource = `export const monacoWorkerFiles = ${JSON.stringify(
    {
      editor: pick('editor.worker-') || '',
      json: pick('json.worker-') || '',
      css: pick('css.worker-') || '',
      html: pick('html.worker-') || '',
      ts: pick('ts.worker-') || '',
    },
    null,
    2,
  )} as const;\n`;
  await ensureDir(path.join(workbenchRoot, 'src', 'generated'));
  await writeFile(path.join(workbenchRoot, 'src', 'generated', 'monacoWorkers.ts'), generatedSource, 'utf8');
}

async function copyStaticFiles() {
  const css = await readFile(path.join(workbenchRoot, 'src', 'workbench.css'), 'utf8');
  await writeFile(path.join(outputRoot, 'app.css'), css, 'utf8');

  for (const iconName of ['icon.svg', 'icon.png', 'icon.ico', 'icon.icns']) {
    await cp(path.join(repoRoot, 'assets', iconName), path.join(outputRoot, iconName));
  }
}

await ensureDir(outputRoot);
await ensureDir(vendorRoot);
await copyVendorAssets();
await copyStaticFiles();

await build({
  entryPoints: [path.join(workbenchRoot, 'src', 'main.ts')],
  outfile: path.join(outputRoot, 'app.js'),
  bundle: true,
  format: 'iife',
  platform: 'browser',
  target: ['es2022'],
  sourcemap: false,
  logLevel: 'info',
});

await rm(bundledOutputRoot, { recursive: true, force: true });
await ensureDir(path.dirname(bundledOutputRoot));
await cp(outputRoot, bundledOutputRoot, { recursive: true });
console.log(`staged workbench assets -> ${path.relative(repoRoot, bundledOutputRoot)}`);
