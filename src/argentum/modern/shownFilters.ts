/**
 * The library filters the Modern window lets you change: Show (edited or not)
 * and Types (RAW or not), in the top bar's view menu. A filter it does not
 * show is one you cannot see or clear, so a value saved for it by the old
 * Classic layout (a rating, a flag, colour labels) would hide photos for no
 * visible reason. Restoring saved filters keeps only the shown ones.
 */
import { EditedStatus, FilterCriteria, FlagStatus, RawStatus } from '../../components/ui/AppProperties';

/** The saved filters, with every filter the window does not show set back to "all". */
export function shownFiltersOnly(saved: Partial<FilterCriteria>): FilterCriteria {
  return {
    rawStatus: saved.rawStatus || RawStatus.All,
    editedStatus: saved.editedStatus || EditedStatus.All,
    rating: 0,
    flagStatus: FlagStatus.All,
    colors: [],
  };
}
