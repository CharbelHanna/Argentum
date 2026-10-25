import { describe, expect, it } from 'vitest';
import { EditedStatus, FlagStatus, RawStatus } from '../../components/ui/AppProperties';
import { shownFiltersOnly } from './shownFilters';

describe('shownFiltersOnly', () => {
  it('keeps the filters the top bar shows', () => {
    const filters = shownFiltersOnly({ rawStatus: RawStatus.RawOnly, editedStatus: EditedStatus.EditedOnly });
    expect(filters.rawStatus).toBe(RawStatus.RawOnly);
    expect(filters.editedStatus).toBe(EditedStatus.EditedOnly);
  });

  it('clears a rating, flag or colour filter that nothing can show or clear', () => {
    const filters = shownFiltersOnly({
      rawStatus: RawStatus.All,
      rating: 3,
      ratingOperator: 'gte',
      flagStatus: FlagStatus.Picked,
      colors: ['red'],
    });
    expect(filters.rating).toBe(0);
    expect(filters.flagStatus).toBe(FlagStatus.All);
    expect(filters.colors).toEqual([]);
    expect(filters).not.toHaveProperty('ratingOperator');
  });

  it('fills in "all" for a filter that was never saved', () => {
    expect(shownFiltersOnly({})).toEqual({
      rawStatus: RawStatus.All,
      editedStatus: EditedStatus.All,
      rating: 0,
      flagStatus: FlagStatus.All,
      colors: [],
    });
  });
});
