import { GIT_BASE_ERROR, type GitBaseErrorCode } from './constants.js';

/** Versão base do modo Comparar: o arquivo no `HEAD` do git, ou o motivo de não haver. */
export type GitHeadResult = { ok: true; source: string } | { ok: false; code: GitBaseErrorCode; message: string };

const MESSAGE: Record<GitBaseErrorCode, string> = {
  [GIT_BASE_ERROR.GIT_UNAVAILABLE]: 'git is not installed or not on PATH',
  [GIT_BASE_ERROR.NOT_A_REPO]: 'the file is not inside a git repository',
  [GIT_BASE_ERROR.NO_COMMITS]: 'the repository has no commits yet',
  [GIT_BASE_ERROR.UNTRACKED]: 'the file is not tracked in git HEAD (new file)',
  [GIT_BASE_ERROR.NOT_UTF8]: 'the file in git HEAD is not valid UTF-8',
  [GIT_BASE_ERROR.OTHER]: 'could not read the file from git HEAD',
};

export const gitHeadError = (code: GitBaseErrorCode): GitHeadResult => ({ ok: false, code, message: MESSAGE[code] });
