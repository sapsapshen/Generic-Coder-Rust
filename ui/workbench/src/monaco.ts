import { monacoWorkerFiles } from './generated/monacoWorkers';

declare global {
  interface Window {
    require?: {
      config?: (settings: { paths: Record<string, string> }) => void;
      (modules: string[], callback: (...args: unknown[]) => void): void;
    };
    monaco?: typeof import('monaco-editor');
    MonacoEnvironment?: unknown;
  }
}

let monacoPromise: Promise<typeof import('monaco-editor')> | null = null;

function selectWorkerFile(label: string): string {
  if (label === 'json') {
    return monacoWorkerFiles.json || monacoWorkerFiles.editor;
  }
  if (label === 'css' || label === 'scss' || label === 'less') {
    return monacoWorkerFiles.css || monacoWorkerFiles.editor;
  }
  if (label === 'html' || label === 'handlebars' || label === 'razor') {
    return monacoWorkerFiles.html || monacoWorkerFiles.editor;
  }
  if (label === 'typescript' || label === 'javascript') {
    return monacoWorkerFiles.ts || monacoWorkerFiles.editor;
  }
  return monacoWorkerFiles.editor;
}

export function loadMonaco(): Promise<typeof import('monaco-editor')> {
  if (window.monaco) {
    return Promise.resolve(window.monaco);
  }
  if (monacoPromise) {
    return monacoPromise;
  }

  monacoPromise = new Promise<typeof import('monaco-editor')>((resolve, reject) => {
    const loader = window.require;
    if (!loader || !loader.config) {
      reject(new Error('Monaco loader is unavailable'));
      return;
    }

    window.MonacoEnvironment = {
      getWorker(_: unknown, label: string) {
        const file = selectWorkerFile(label);
        if (!file) {
          throw new Error(`Missing Monaco worker for ${label}`);
        }
        return new Worker(`/static/vendor/monaco/vs/assets/${file}`, {
          name: label,
        });
      },
    };

    loader.config({ paths: { vs: '/static/vendor/monaco/vs' } });
    loader(['vs/editor/editor.main'], () => {
      if (!window.monaco) {
        reject(new Error('Monaco failed to initialize'));
        return;
      }
      resolve(window.monaco);
    });
  }).catch((error) => {
    monacoPromise = null;
    throw error;
  });

  return monacoPromise!;
}
