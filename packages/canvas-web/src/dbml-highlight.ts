/** Realce de sintaxe DBML por linha para o editor (camada atrás do textarea). */

export type TokKind = 'kw' | 'ty' | 'attr' | 'num' | 'cm' | 'punc' | 'plain';
export interface Tok {
  text: string;
  kind: TokKind;
}

const KW = /^(table|tablegroup|tablepartial|ref|enum|note|project|indexes)$/i;
const TY =
  /^(int|integer|bigint|smallint|varchar|char|text|timestamp|timestamptz|date|time|decimal|numeric|float|double|boolean|bool|uuid|json|jsonb|serial|bytea)$/i;
const AT = /^(pk|primary key|unique|not null|null|ref|headercolor|color|increment|default|note|as)$/i;
const RX = new RegExp(
  '(//.*$|\'[^\']*\'|"[^"]*"|`[^`]*`|\\b(?:Table|TableGroup|TablePartial|Ref|Enum|Note|Project|Indexes)\\b' +
    '|\\b(?:int|integer|bigint|smallint|varchar|char|text|timestamp|timestamptz|date|time|decimal|numeric|float|double|boolean|bool|uuid|json|jsonb|serial|bytea)\\b' +
    '|\\b(?:primary key|not null|pk|unique|null|ref|headercolor|color|increment|default|note|as)\\b' +
    '|#[0-9a-fA-F]{3,8}\\b|\\b\\d+(?:\\.\\d+)?\\b|[{}()\\[\\]<>:,;.-])',
  'i'
);

export function tokenizeLine(line: string): Tok[] {
  return line
    .split(RX)
    .filter((s) => s)
    .map((text): Tok => {
      let kind: TokKind = 'plain';
      if (text.startsWith('//')) kind = 'cm';
      else if (KW.test(text)) kind = 'kw';
      else if (TY.test(text)) kind = 'ty';
      else if (AT.test(text)) kind = 'attr';
      else if (/^(#|\d)/.test(text)) kind = 'num';
      else if (/^[{}()[\]<>:,;.-]$/.test(text)) kind = 'punc';
      return { text, kind };
    });
}

/** Intervalo [início, fim] (índices de linha, base 0) do bloco `Table` da tabela dada, ou null. */
export function tableLineRange(lines: string[], schema: string | null | undefined, name: string): [number, number] | null {
  const head = /^\s*table\s+(?:"?([^\s".{[]+)"?\.)?"?([^\s"{[]+)"?/i;
  for (let i = 0; i < lines.length; i++) {
    const m = head.exec(lines[i]);
    if (!m || m[2] !== name || (schema && m[1] !== schema)) continue;
    let j = i;
    while (j < lines.length && lines[j].trim() !== '}') j++;
    return [i, Math.min(j, lines.length - 1)];
  }
  return null;
}
