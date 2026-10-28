import { describe, expect, it } from 'vitest';
import { fileNameOf, isVirtualCopyPath, physicalPathOf, VIRTUAL_COPY } from './paths';

describe('a photo path', () => {
  it('names the file after the last slash or backslash', () => {
    expect(fileNameOf('/p/IMG.CR2')).toBe('IMG.CR2');
    expect(fileNameOf('C:\\p\\IMG.CR2')).toBe('IMG.CR2');
    expect(fileNameOf('IMG.CR2')).toBe('IMG.CR2');
  });

  it('tells a virtual copy from its photo, and leads back to the photo on disk', () => {
    const copy = `/p/IMG.CR2${VIRTUAL_COPY}a1b2c3`;
    expect(isVirtualCopyPath(copy)).toBe(true);
    expect(isVirtualCopyPath('/p/IMG.CR2')).toBe(false);
    expect(physicalPathOf(copy)).toBe('/p/IMG.CR2');
    expect(physicalPathOf('/p/IMG.CR2')).toBe('/p/IMG.CR2');
  });

  it("names a virtual copy's file after its photo", () => {
    expect(fileNameOf(physicalPathOf('C:\\p\\IMG.CR2?vc=a1b2c3'))).toBe('IMG.CR2');
  });
});
