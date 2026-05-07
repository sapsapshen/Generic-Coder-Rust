export function escapeHtml(text: unknown): string {
  const raw = text == null ? '' : String(text);
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function inferLanguage(filePath: string): string {
  const extension = filePath.split('.').pop()?.toLowerCase();
  switch (extension) {
    case 'rs':
      return 'rust';
    case 'ts':
    case 'tsx':
      return 'typescript';
    case 'js':
    case 'mjs':
    case 'cjs':
      return 'javascript';
    case 'json':
      return 'json';
    case 'md':
      return 'markdown';
    case 'toml':
      return 'ini';
    case 'yml':
    case 'yaml':
      return 'yaml';
    case 'html':
      return 'html';
    case 'css':
      return 'css';
    case 'sh':
      return 'shell';
    case 'diff':
    case 'patch':
      return 'diff';
    default:
      return 'plaintext';
  }
}
