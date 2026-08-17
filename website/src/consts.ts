// Central repo / product facts reused across components.
const REPO_OWNER = 'ben-burwood';
const REPO_NAME = 'anybucket';
export const REPO_URL = `https://github.com/${REPO_OWNER}/${REPO_NAME}`;
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LATEST_RELEASE_URL = `${RELEASES_URL}/latest`;
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;
export const API_REPO = `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}`;
export const API_LATEST_RELEASE = `${API_REPO}/releases/latest`;

export const WINGET_ID = 'BenBurwood.AnyBucket';

// GitHub Pages serves the site under a base path (see astro.config.mjs).
const BASE = import.meta.env.BASE_URL.replace(/\/$/, '');
export const asset = (path: string) => `${BASE}${path}`;

export interface Platform {
  id: 'windows' | 'macos' | 'linux';
  name: string;
  note: string;
  /** Download instruction shown on the card (Windows uses the WinGet block instead). */
  blurb: string;
  /** Lowercase substrings matched against the user agent, in priority order. */
  uaHints: string[];
  /** Lowercase release-asset filename suffixes, in preference order. */
  assetMatch: string[];
}

export const PLATFORMS: Platform[] = [
  {
    id: 'windows',
    name: 'Windows',
    note: 'Windows 10 & 11 · x64',
    blurb: '',
    uaHints: ['win'],
    assetMatch: ['-setup.exe', '.exe', '.msi'],
  },
  {
    id: 'macos',
    name: 'macOS',
    note: 'Apple Silicon (M-series)',
    blurb: 'Download the .dmg for Apple Silicon and open it to install.',
    uaHints: ['mac', 'iphone', 'ipad'],
    assetMatch: ['.dmg', '.app.tar.gz'],
  },
  {
    id: 'linux',
    name: 'Linux',
    note: 'AppImage & .deb · x64',
    blurb: 'Download the .AppImage or .deb package and open it to install.',
    uaHints: ['linux', 'android', 'x11'],
    assetMatch: ['.appimage', '.deb'],
  },
];
