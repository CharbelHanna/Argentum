/**
 * One History step per drag, recorded when the slider is let go. Ours.
 *
 * Their editor records a step 500 ms after the last change (debouncedSetHistory
 * in src/hooks/useEditorActions.ts), whether or not the mouse is still down. So
 * a drag that paused now and then left a step at every pause: eight "LUT
 * Intensity" steps for one drag.
 *
 * While isSliderDragging is on, the step they ask for is held here instead, and
 * letting go records the latest one at once. Their sliders, curves, colour
 * wheels and the crop's pan and wheel-zoom all set that flag, so each of them
 * becomes one step. A change made without a drag (a click, a typed value, a
 * double-click reset) keeps their 500 ms.
 *
 * Installed at import by replacing the store's pushHistory, which only their
 * debounce calls, so no line of theirs changes.
 */

import { useEditorStore } from '../store/useEditorStore';
import { debouncedSetHistory } from '../hooks/useEditorActions';
import type { Adjustments } from '../utils/adjustments';

const recordStep = useEditorStore.getState().pushHistory;

/** The latest step asked for during the drag, and the photo it belongs to. */
let held: { path: string | undefined; adjustments: Adjustments } | null = null;

useEditorStore.setState({
  pushHistory: (adjustments: Adjustments) => {
    const { isSliderDragging, selectedImage } = useEditorStore.getState();
    if (isSliderDragging) {
      held = { path: selectedImage?.path, adjustments };
      return;
    }
    held = null;
    recordStep(adjustments);
  },
});

useEditorStore.subscribe((state, prev) => {
  if (!prev.isSliderDragging || state.isSliderDragging) return;
  // A change in the drag's last 500 ms is still waiting in their debounce:
  // recorded now, through the pushHistory above, and newer than anything held.
  debouncedSetHistory.flush();
  // A photo opened mid-drag started a new history; the held step was the old photo's.
  if (held && held.path === state.selectedImage?.path) recordStep(held.adjustments);
  held = null;
});
