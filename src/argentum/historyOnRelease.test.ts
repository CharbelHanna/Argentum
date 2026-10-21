import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useEditorStore } from '../store/useEditorStore';
import { debouncedSetHistory } from '../hooks/useEditorActions';
import { INITIAL_ADJUSTMENTS, type Adjustments } from '../utils/adjustments';
import type { SelectedImage } from '../components/ui/AppProperties';
import './historyOnRelease';

const photo = (path: string) => ({ path }) as SelectedImage;
const withExposure = (exposure: number): Adjustments => ({ ...INITIAL_ADJUSTMENTS, exposure });
const steps = () => useEditorStore.getState().history.length - 1;

/** What their setAdjustments does on every change. */
const change = (exposure: number) => {
  const adjustments = withExposure(exposure);
  useEditorStore.setState({ adjustments });
  debouncedSetHistory(adjustments);
};
const drag = (on: boolean) => useEditorStore.setState({ isSliderDragging: on });

describe('history on release', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useEditorStore.setState({ selectedImage: photo('a.raf'), isSliderDragging: false });
    useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS);
  });
  afterEach(() => {
    debouncedSetHistory.cancel();
    vi.useRealTimers();
  });

  it('records nothing while held, however long the pause, and one step on release', () => {
    drag(true);
    change(0.2);
    vi.advanceTimersByTime(2000);
    change(0.5);
    vi.advanceTimersByTime(2000);
    expect(steps()).toBe(0);
    drag(false);
    expect(steps()).toBe(1);
    expect(useEditorStore.getState().history[1].exposure).toBe(0.5);
  });

  it('records a change made just before release at once, and only once', () => {
    drag(true);
    change(0.2);
    vi.advanceTimersByTime(2000);
    change(0.7);
    drag(false);
    vi.advanceTimersByTime(2000);
    expect(steps()).toBe(1);
    expect(useEditorStore.getState().history[1].exposure).toBe(0.7);
  });

  it('records nothing for a drag that changed nothing', () => {
    drag(true);
    vi.advanceTimersByTime(2000);
    drag(false);
    expect(steps()).toBe(0);
  });

  it('keeps their 500 ms for a change without a drag', () => {
    change(0.3);
    expect(steps()).toBe(0);
    vi.advanceTimersByTime(500);
    expect(steps()).toBe(1);
  });

  it('drops a step held for a photo that is no longer open', () => {
    drag(true);
    change(0.4);
    vi.advanceTimersByTime(2000);
    useEditorStore.setState({ selectedImage: photo('b.raf') });
    useEditorStore.getState().resetHistory(INITIAL_ADJUSTMENTS);
    drag(false);
    expect(steps()).toBe(0);
  });
});
