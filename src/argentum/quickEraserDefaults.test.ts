// A new Quick Eraser starts at RapidRAW 1.6.7's sizes (b407d348); one already
// in an edit keeps the values stored in it.
import { describe, expect, it } from 'vitest';
import { createSubMask } from '../utils/maskUtils';
import { Mask } from '../components/panel/right/Masks';

describe('a new Quick Eraser', () => {
  it('grows by 150 and feathers by 75', () => {
    const { parameters } = createSubMask(Mask.QuickEraser, { width: 6000, height: 4000 });
    expect(parameters).toMatchObject({ grow: 150, feather: 75 });
  });
});
