/**
 * The library filters the Modern window lets you change: Show (edited or not)
 * and Types (RAW or not), in the top bar's view menu. A filter it does not
 * show is one you cannot see or clear, so a value saved for it by the old
 * Classic layout (a rating, a flag, colour labels) would hide photos for no
 * visible reason. Restoring saved filters keeps only the shown ones.
 */
import { EditedStatus, FilterCriteria, FlagStatus, RawStatus } from '../../components/ui/AppProperties';
import type { AgKey } from '../locales';

/** The view menu's Show choices, each value with its words. */
export const SHOW_CHOICES = [
  [EditedStatus.All, 'mwShowAll'],
  [EditedStatus.EditedOnly, 'mwShowEdited'],
  [EditedStatus.UneditedOnly, 'mwShowUnedited'],
] as const;

/** The view menu's File types choices, each value with its words. */
export const TYPE_CHOICES = [
  [RawStatus.All, 'mwTypesAll'],
  [RawStatus.RawOnly, 'mwTypesRaw'],
  [RawStatus.NonRawOnly, 'mwTypesOther'],
] as const;

/** The words for each filter that is hiding photos, or nothing when every photo shows. */
export function activeFilterNames(filters: FilterCriteria, t: (key: AgKey) => string): string {
  const show = SHOW_CHOICES.find(([value]) => value !== EditedStatus.All && value === filters.editedStatus);
  const type = TYPE_CHOICES.find(([value]) => value !== RawStatus.All && value === filters.rawStatus);
  return [show, type].flatMap((choice) => (choice ? [t(choice[1])] : [])).join(' · ');
}

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
