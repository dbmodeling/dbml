/** Geist e Geist Mono embutidas (licença OFL em `fonts/Geist-LICENSE.txt`): sem CDN, funciona offline e no webview do VS Code. */
import geist400 from './fonts/Geist-Regular.ttf?inline';
import geist500 from './fonts/Geist-Medium.ttf?inline';
import geist600 from './fonts/Geist-SemiBold.ttf?inline';
import mono400 from './fonts/GeistMono-Regular.ttf?inline';
import mono500 from './fonts/GeistMono-Medium.ttf?inline';

const face = (family: string, weight: number, src: string): string =>
  `@font-face{font-family:'${family}';font-weight:${weight};font-style:normal;font-display:swap;src:url(${src}) format('truetype');}`;

export const FONT_FACE_CSS = [
  face('Geist', 400, geist400),
  face('Geist', 500, geist500),
  face('Geist', 600, geist600),
  face('Geist Mono', 400, mono400),
  face('Geist Mono', 500, mono500),
].join('\n');
