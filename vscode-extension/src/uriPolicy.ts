/** Parte de `vscode.Uri` que a política precisa (permite testar sem o host). */
export interface UriLike {
  scheme: string;
  authority: string;
  path: string;
}

/**
 * Só `.dbml` que pertence a uma pasta do workspace pode ser aberto a pedido da Webview.
 * Impede que a Webview mande o host abrir `http:`, `command:` ou arquivos arbitrários.
 */
export function isAllowedDbmlUri(uri: UriLike, folders: readonly UriLike[]): boolean {
  if (!uri.path.toLowerCase().endsWith('.dbml')) return false;
  if (uri.path.split('/').includes('..')) return false;
  return folders.some((f) => {
    if (f.scheme !== uri.scheme || f.authority !== uri.authority) return false;
    const base = f.path.endsWith('/') ? f.path : `${f.path}/`;
    return uri.path.startsWith(base);
  });
}
