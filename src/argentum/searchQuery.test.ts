// Which search tags are read as EXIF queries (ADVANCED_QUERY_REGEX in hooks/useSortedLibrary.ts).
// RapidRAW 1c2f1d64, adapted.
import { describe, expect, it } from 'vitest';
import { ADVANCED_QUERY_REGEX } from '../hooks/useSortedLibrary';

const query = (tag: string) => {
  const m = tag.match(ADVANCED_QUERY_REGEX);
  return m ? { field: m[1].toLowerCase(), operator: m[2] || '=', value: m[3] } : null;
};

describe('a search tag', () => {
  it('that only starts like a field is a plain tag', () => {
    for (const tag of ['summer', 'street', 'family', 'friends', 'isolated', 'lensball', 'models', 'Fog']) {
      expect(query(tag), tag).toBeNull();
    }
  });

  it('in the forms the search tooltip shows is a query', () => {
    expect(query('iso:>800')).toEqual({ field: 'iso', operator: '>', value: '800' });
    expect(query('f:<=2.8')).toEqual({ field: 'f', operator: '<=', value: '2.8' });
    expect(query('s:1/200')).toEqual({ field: 's', operator: '=', value: '1/200' });
    expect(query('mm:50')).toEqual({ field: 'mm', operator: '=', value: '50' });
    expect(query('camera:sony')).toEqual({ field: 'camera', operator: '=', value: 'sony' });
  });

  it('with an operator and no colon, or a space, is still a query, as before', () => {
    expect(query('iso>800')).toEqual({ field: 'iso', operator: '>', value: '800' });
    expect(query('rating >= 4')).toEqual({ field: 'rating', operator: '>=', value: '4' });
    expect(query('camera sony')).toEqual({ field: 'camera', operator: '=', value: 'sony' });
    expect(query('lens 50mm')).toEqual({ field: 'lens', operator: '=', value: '50mm' });
  });
});
