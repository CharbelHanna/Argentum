/**
 * Suggested for this photo: the LUTs the open photo is likely to get,
 * learned from the LUTs put on photos before. Ours.
 *
 * The learning is src-tauri/src/mods/look_picks.rs: a LUT put on the open
 * photo is a pick, kept with the photo's light and colours, and a photo's
 * suggestions are the LUTs picked for the photos most like it. Here: asking
 * for them when a photo is ready, and telling it when a LUT goes on or off.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { ag } from '../ag';

export type Light = 'lamp' | 'daylight';

/** look_picks.rs `Suggestion`. */
export interface Suggestion {
  /** The LUT's path. */
  lut: string;
  /** How many of the photo's nearest picks in its light chose it. */
  votes: number;
  /** How many photos it was picked for, in all. */
  picks: number;
  light: Light;
}

/** look_picks.rs `Suggested`. */
export interface Suggested {
  learning: boolean;
  /** How many photos have a pick. */
  picks: number;
  light: Light;
  /** Most likely first; empty while learning is off or picks are too few. */
  suggestions: Suggestion[];
  /** Picks that took one of the three shown, out of those made while there were some. */
  right: number;
  of: number;
}

/** look_picks.rs `MIN_PICKS` and `SHOWN`. */
export const MIN_PICKS = 3;
export const SHOWN = 3;

/**
 * The open photo's suggestions, once it is decoded (the backend reads it), and
 * what to tell the backend. An answer for a photo no longer open is dropped.
 */
export function useSuggested(photo: string | null, ready: boolean) {
  const [suggested, setSuggested] = useState<Suggested | null>(null);
  const current = useRef(photo);
  current.current = photo;

  const take = useCallback(
    (forPhoto: string, request: Promise<Suggested>) =>
      request
        .then((answer) => {
          if (current.current === forPhoto) setSuggested(answer);
          return answer;
        })
        .catch((err) => {
          console.warn('Looks: suggestions:', err);
          return null;
        }),
    [],
  );

  // The last photo's answer stays until this one's comes, so the list below
  // does not jump while a photo decodes.
  const ask = useCallback(() => {
    if (!photo) setSuggested(null);
    else if (ready) take(photo, ag<Suggested>('look_suggestions', { path: photo }));
  }, [photo, ready, take]);

  useEffect(ask, [ask]);

  /** A LUT went on the open photo. */
  const lutWentOn = (lut: string) =>
    photo ? take(photo, ag<Suggested>('look_pick', { path: photo, lut })) : Promise.resolve(null);

  /** The open photo's LUT came off. */
  const lutCameOff = () => (photo ? take(photo, ag<Suggested>('look_unpick', { path: photo })) : Promise.resolve(null));

  const setLearning = (on: boolean) => ag('set_look_learning', { on }).then(ask);

  const forget = () => ag('forget_look_picks').then(ask);

  return { suggested, lutWentOn, lutCameOff, setLearning, forget };
}
