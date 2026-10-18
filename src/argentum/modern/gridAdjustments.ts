/**
 * The settings of the photo selected in the Modern grid, read from its sidecar
 * without opening it, for the Edit tab to show read-only. Ours.
 *
 * The same read the editor makes when it opens a photo (load_metadata, then
 * normalizeLoadedAdjustments), and nothing else: no decode, no preview. The
 * latest selection wins: a slower read for a photo no longer selected is dropped.
 */

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Invokes } from '../../components/ui/AppProperties';
import { type Adjustments, INITIAL_ADJUSTMENTS, normalizeLoadedAdjustments } from '../../utils/adjustments';

export function useGridAdjustments(path: string | null): Adjustments | null {
  const [read, setRead] = useState<{ path: string; adjustments: Adjustments } | null>(null);

  useEffect(() => {
    if (!path) return;
    let current = true;
    invoke<{ adjustments?: (Adjustments & { is_null?: boolean }) | null }>(Invokes.LoadMetadata, { path })
      .then((metadata) => {
        if (!current) return;
        const saved = metadata?.adjustments;
        const adjustments = saved && !saved.is_null ? normalizeLoadedAdjustments(saved) : { ...INITIAL_ADJUSTMENTS };
        setRead({ path, adjustments });
      })
      .catch(() => current && setRead({ path, adjustments: { ...INITIAL_ADJUSTMENTS } }));
    return () => {
      current = false;
    };
  }, [path]);

  return path && read?.path === path ? read.adjustments : null;
}
