/**
 * The per-photo map Relight and Fog need, made when their switch goes on. Ours.
 *
 * Relight needs a map of which way each surface faces, Fog a depth map. Each is
 * worked out from the photo by a model RapidRAW's commands download the first
 * time (`generate_relight_normal_map`, `generate_full_image_depth_map` in their
 * ai_commands.rs). Their panel calls the command at once (Effects.tsx,
 * handleRelightToggle, main at 454789b7), so the download starts unasked.
 * Here the switch first looks at what is on disk (Settings > AI and models'
 * listing, mods/model_manager.rs) and, if anything is missing, asks in the
 * panel with its size, as the Subject and Sky masks do
 * (argentum/selection/selection.tsx).
 *
 * The answer belongs to the photo it was asked for: switching photos while
 * the model works drops it, rather than putting one photo's map on another.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'react-toastify';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { ag } from '../ag';

/** What one effect needs: the models its command fetches (ids in mods/model_catalog.rs) and the command. */
export interface MapSource {
  models: readonly string[];
  command: string;
}

export const RELIGHT_MAP: MapSource = { models: ['relight'], command: 'generate_relight_normal_map' };

/**
 * Their depth command loads all four mask models together (get_or_init_ai_models
 * in ai_processing.rs), so any of them missing is a download.
 */
export const DEPTH_MAP: MapSource = {
  models: ['subject', 'foreground', 'sky', 'depth'],
  command: 'generate_full_image_depth_map',
};

/** Bytes the command would download first: 0 when every model is on disk. */
async function bytesToFetch(models: readonly string[]): Promise<number> {
  const listing = await ag('list_ai_models');
  return listing.models
    .filter((m) => models.includes(m.id) && m.onDisk !== 'downloaded')
    .reduce((sum, m) => sum + (m.downloadBytes ?? 0), 0);
}

export type MapPhase = { kind: 'idle' } | { kind: 'asking'; bytes: number } | { kind: 'working' };

const IDLE: MapPhase = { kind: 'idle' };

interface EffectState {
  enabled: boolean;
  hasMap: boolean;
  /** Switches the effect on, with this photo's map when it was just made. */
  turnOn(map?: string): void;
  turnOff(): void;
}

/**
 * The effect's switch and its map: `toggle` is the switch, `download` and
 * `notNow` the ask's buttons. A model that fails switches the effect off.
 */
export function useEffectMap(source: MapSource, { enabled, hasMap, turnOn, turnOff }: EffectState) {
  const path = useEditorStore((s) => s.selectedImage?.path ?? null);
  const isEditorPhoto = useEditorStore((s) => !!s.selectedImage?.isReady);
  const isEditorView = useUIStore((s) => s.activeView === 'editor');
  const [state, setState] = useState<{ path: string | null; phase: MapPhase }>({ path: null, phase: IDLE });
  // Each start or stop outdates the answers to the ones before it.
  const request = useRef(0);

  const download = useCallback(async () => {
    const id = ++request.current;
    const forPath = useEditorStore.getState().selectedImage?.path ?? null;
    setState({ path: forPath, phase: { kind: 'working' } });
    try {
      const map = await invoke<string>(source.command);
      if (request.current === id && useEditorStore.getState().selectedImage?.path === forPath) turnOn(map);
    } catch (err) {
      if (request.current === id) {
        toast.error(String(err));
        turnOff();
      }
    } finally {
      if (request.current === id) setState({ path: forPath, phase: IDLE });
    }
  }, [source.command, turnOn, turnOff]);

  const start = useCallback(async () => {
    const id = ++request.current;
    const forPath = useEditorStore.getState().selectedImage?.path ?? null;
    let bytes = 0;
    try {
      bytes = await bytesToFetch(source.models);
    } catch {
      // Not knowing is not a reason to stop: the command still says if it fails.
    }
    if (request.current !== id) return;
    if (bytes > 0) setState({ path: forPath, phase: { kind: 'asking', bytes } });
    else download();
  }, [source.models, download]);

  const notNow = useCallback(() => {
    request.current++;
    setState({ path: null, phase: IDLE });
    turnOff();
  }, [turnOff]);

  const toggle = (on: boolean) => {
    if (!on) notNow();
    else if (hasMap) turnOn();
    else start();
  };

  // A phase is for the photo it began on.
  const phase = state.path === path ? state.phase : IDLE;

  // On without its map, as when settings are pasted from another photo (the
  // map is never copied): make this photo's, once, while it is open.
  const autoStarted = useRef<string | null>(null);
  useEffect(() => {
    if (!enabled || hasMap || !isEditorPhoto || !isEditorView || phase.kind !== 'idle') return;
    if (autoStarted.current === path) return;
    autoStarted.current = path;
    start();
  }, [enabled, hasMap, isEditorPhoto, isEditorView, phase.kind, path, start]);

  return { phase, toggle, download, notNow, isOn: enabled || phase.kind !== 'idle' };
}
