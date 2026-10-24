/**
 * Is there a newer Argentum? Asks GitHub for the latest **published release**
 * of akhater/Argentum, at start and every six hours. Ours.
 *
 * Classic's library header asked once at start and showed "update available"
 * (RapidRAW's notifier, pointed at our repository). It went with that header
 * when the Classic layout was deleted (PR #117, 2026-10-20), so for a while
 * nothing said a release was out. This brings it back for the Modern window.
 *
 * The comparison and the timing are Shoebox's (`src-tauri/src/updates.rs`):
 * only a published release counts (GitHub's "latest" leaves out drafts and
 * pre-releases), versions compare as numbers, a tag that isn't a version never
 * announces anything, and a failed check is retried sooner while keeping what
 * the last good one found. Nothing is downloaded: the window shows the new
 * version and opens its release page. The request carries no photo data.
 */

import { getVersion } from '@tauri-apps/api/app';
import { useEffect, useSyncExternalStore } from 'react';

/** Where a new version is downloaded from. */
export const RELEASES_PAGE = 'https://github.com/akhater/Argentum/releases/latest';
const LATEST_API = 'https://api.github.com/repos/akhater/Argentum/releases/latest';
/** How often a running Argentum asks again. */
const EVERY_MS = 6 * 3600 * 1000;
/** How soon after a failed check (offline, GitHub busy) it tries again. */
const RETRY_MS = 30 * 60 * 1000;

export interface UpdateStatus {
  /** This Argentum ("26.43.4"); null until known. */
  readonly current: string | null;
  /** The latest published release; null if none is known. */
  readonly latest: string | null;
  /** `latest` is newer than `current`. */
  readonly newer: boolean;
  /** The release's page on GitHub. */
  readonly url: string;
}

/** "v26.43.1", "Argentum v26.43.1" -> "26.43.1". */
export function cleanTag(tag: string): string {
  return tag.trim().replace(/^[^0-9]*/, '');
}

/** "26.43.1" -> [26, 43, 1]. A suffix ("-beta", "+build") is set aside. */
function numbers(version: string): number[] | null {
  const core = cleanTag(version).split(/[-+]/)[0];
  if (!core) return null;
  const parts = core.split('.').map((n) => (/^\d+$/.test(n) ? Number(n) : NaN));
  return parts.some(Number.isNaN) ? null : parts;
}

/**
 * `latest` is a later version than `current` ("26.43.1" > "26.42.9";
 * "26.43" = "26.43.0"). Anything that isn't a version is never newer.
 * Shoebox's `is_newer`, in TypeScript.
 */
export function isNewer(latest: string, current: string): boolean {
  const l = numbers(latest);
  const c = numbers(current);
  if (!l || !c) return false;
  const n = Math.max(l.length, c.length);
  for (let i = 0; i < n; i++) {
    const a = l[i] ?? 0;
    const b = c[i] ?? 0;
    if (a !== b) return a > b;
  }
  return false;
}

let status: UpdateStatus = { current: null, latest: null, newer: false, url: RELEASES_PAGE };
const listeners = new Set<() => void>();
let started = false;

function publish(next: UpdateStatus): void {
  status = next;
  for (const listener of listeners) listener();
}

async function check(): Promise<boolean> {
  const current = status.current ?? (await getVersion().catch(() => null));
  try {
    const response = await fetch(LATEST_API, { headers: { Accept: 'application/vnd.github+json' } });
    if (response.status === 404) {
      publish({ ...status, current });
      return true;
    }
    if (!response.ok) throw new Error(`GitHub answered ${response.status}`);
    const release: { tag_name?: string; html_url?: string } = await response.json();
    const latest = release.tag_name ? cleanTag(release.tag_name) : null;
    // Only ever a page of this repository.
    const url = release.html_url?.startsWith('https://github.com/akhater/Argentum/') ? release.html_url : RELEASES_PAGE;
    publish({ current, latest, newer: !!latest && !!current && isNewer(latest, current), url });
    return true;
  } catch {
    // Keep what the last good check found: an update seen this morning still shows offline.
    publish({ ...status, current });
    return false;
  }
}

function start(): void {
  if (started) return;
  started = true;
  const loop = async () => {
    const ok = await check();
    setTimeout(loop, ok ? EVERY_MS : RETRY_MS);
  };
  void loop();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

const getSnapshot = (): UpdateStatus => status;

/** This version and whether a newer release is out. The first caller starts the checks. */
export function useUpdateStatus(): UpdateStatus {
  useEffect(start, []);
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
