// Shoebox's tests for the same comparison (src-tauri/src/updates.rs), in TypeScript.
import { describe, expect, it } from 'vitest';
import { cleanTag, isNewer } from './updates';

describe('a newer release', () => {
  it('compares versions as numbers', () => {
    expect(isNewer('26.43.1', '26.42.1')).toBe(true);
    expect(isNewer('v26.42.2', '26.42.1')).toBe(true);
    expect(isNewer('26.42.10', '26.42.9')).toBe(true);
    expect(isNewer('27.1.0', '26.52.3')).toBe(true);
    expect(isNewer('26.42.1', '26.42.1')).toBe(false);
    expect(isNewer('v26.42.1', '26.42.1')).toBe(false);
    expect(isNewer('26.41.5', '26.42.1')).toBe(false);
    expect(isNewer('26.42', '26.42.0')).toBe(false);
    expect(isNewer('26.42.0.1', '26.42')).toBe(true);
  });

  it('never comes from a tag that is not a version', () => {
    expect(isNewer('', '26.42.1')).toBe(false);
    expect(isNewer('latest', '26.42.1')).toBe(false);
    expect(isNewer('v26.x', '26.42.1')).toBe(false);
    expect(isNewer('Argentum v26.43.1', '26.42.1')).toBe(true);
    expect(isNewer('26.43.0-beta', '26.42.1')).toBe(true);
  });

  it('a development build one ahead of the release stays quiet', () => {
    expect(isNewer('26.43.3', '26.43.4')).toBe(false);
  });

  it('tags lose their prefix', () => {
    expect(cleanTag('v26.43.1')).toBe('26.43.1');
    expect(cleanTag('Argentum v26.43.1')).toBe('26.43.1');
    expect(cleanTag('26.43.1')).toBe('26.43.1');
  });
});
