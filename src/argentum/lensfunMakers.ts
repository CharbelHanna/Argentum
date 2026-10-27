/**
 * The lens makers in the lensfun database, asked for once. Ours.
 *
 * The list is fixed while the app runs (lens_correction.rs reads the database
 * at start), yet the crop panel and My Gear each asked Rust for it whenever
 * they opened. The first answer is kept; a failed ask (the database still
 * loading) is not, so the next one tries again.
 */

import { invoke } from '@tauri-apps/api/core';

let makers: Promise<string[]> | null = null;

/** The makers, sorted, from the first answer that came back. */
export function lensfunMakers(): Promise<string[]> {
  if (!makers) {
    makers = invoke<string[]>('get_lensfun_makers').catch((error) => {
      makers = null;
      throw error;
    });
  }
  return makers;
}
