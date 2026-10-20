/**
 * The camera's built-in lens correction switch, at the top of Crop > Lens
 * Correction. Ours. Draws nothing for a photo that carries no correction.
 *
 * While it is on, the lensfun profile below it is not applied (see
 * mods/lens_builtin.rs), and the Distortion, Chromatic Aberration and
 * Vignetting switches and amounts act on the camera's correction.
 */

import { Camera } from 'lucide-react';
import Switch from '../components/ui/Switch';
import type { Adjustments } from '../utils/adjustments';
import { useAgTranslation } from './locales';
import { builtinTables, withBuiltin } from './lensBuiltin';

interface LensBuiltinSwitchProps {
  adjustments: Adjustments;
  setAdjustments(update: (prev: Adjustments) => Adjustments): void;
}

export default function LensBuiltinSwitch({ adjustments, setAdjustments }: LensBuiltinSwitchProps) {
  const agT = useAgTranslation();
  const tables = builtinTables(adjustments);
  if (!tables) return null;

  const on = (adjustments as Adjustments & { lensBuiltin?: boolean }).lensBuiltin === true;
  // "Fujifilm built-in" from the reader; the make is what the line names.
  const make = tables.source.replace(/ built-in$/, '');

  return (
    <div className="bg-bg-primary p-3 rounded-md border border-surface">
      <div className="flex items-center gap-3">
        <Camera size={16} className="text-text-secondary shrink-0" />
        <Switch
          className="grow"
          label={agT('lensBuiltin')}
          checked={on}
          onChange={(value) => setAdjustments((prev) => withBuiltin(prev, value))}
        />
      </div>
      <p className="text-xs text-text-secondary mt-1 ml-7">{`${make}, ${agT('lensBuiltinFrom')}`}</p>
    </div>
  );
}
