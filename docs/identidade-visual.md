# Identidade visual — DB Modeling

<img src="../crates/app/assets/brand/icon-128.png" alt="Marca do DB Modeling" width="72" height="72" />

A marca representa duas tabelas brancas conectadas por uma relação, sobre um
quadrado arredondado com base laranja `#e56731` e brilho suave. O mesmo laranja
destaca ações e seleções na interface.

## Arquivos da marca

| Arquivo | Uso |
|---|---|
| [icon.svg](../crates/app/assets/brand/icon.svg) | Matriz vetorial de 512 × 512 |
| [icon-128.png](../crates/app/assets/brand/icon-128.png) | Versão raster para documentação e templates |
| [icon.ico](../crates/app/assets/brand/icon.ico) | Ícone do executável Windows |
| [mark.svg](../crates/app/assets/brand/mark.svg) | Glifo sem fundo, monocromático via `currentColor` |
| [favicon.svg](../favicon.svg) | Ícone usado na apresentação do repositório |

Após alterar a matriz SVG, regenere os PNGs e o ICO com
`cargo run --manifest-path tools/render-icons/Cargo.toml` e confira as cópias do
ícone na raiz e em [vscode-extension/media](../vscode-extension/media/).
Use o glifo quando o contexto pedir uma marca monocromática e mantenha a
proporção original ao redimensionar.

## Paleta do workbench

As cores abaixo são as bases dos temas sólidos. O modo translúcido usa
variações de opacidade definidas nos tokens.

| Papel | Escuro | Claro |
|---|---|---|
| Destaque | `#e56731` | `#e56731` |
| Barra superior | `#0e0f11` | `#f3f3f1` |
| Superfície do diagrama | `#0a0b0c` | `#fbfbfa` |
| Cartão de tabela | `#17181b` | `#ffffff` |
| Texto | `#ececee` | `#17181b` |
| Texto secundário | `#a3a5ab` | `#55585f` |

Para tabelas e grupos, a paleta compartilhada é azul `#589aed`, ciano
`#00b1ba`, verde `#4eb068`, lima `#8ea432`, âmbar `#c09000`, laranja
`#df784e`, rosa `#d771a4` e violeta `#9f85e5`.

Os tokens ficam em [glass.rs](../crates/app/src/glass.rs) e
[glass.ts](../packages/canvas-web/src/glass.ts). A paleta do diagrama é
definida em [layout](../crates/core/layout/src/lib.rs).

## Tipografia

**Geist** atende à interface com pesos 400, 500 e 600. **Geist Mono** atende
à edição DBML e aos dados monoespaçados com pesos 400 e 500. Desktop e Web
embutem as fontes sob a licença OFL.

## Referências

Veja o [guia do workbench](05-ux.md) para capturas dos temas claro e escuro e
do editor no modo dividido.
