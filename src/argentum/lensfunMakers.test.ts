// The lensfun makers are asked for once (argentum/lensfunMakers.ts); a failed
// ask, the database still loading, is asked again.
import { describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { lensfunMakers } = await import('./lensfunMakers');

describe('the lensfun makers', () => {
  it('are asked again after a failure, then kept', async () => {
    invoke.mockRejectedValueOnce('Lens database not loaded');
    await expect(lensfunMakers()).rejects.toBe('Lens database not loaded');

    invoke.mockResolvedValueOnce(['Canon', 'Nikon']);
    expect(await lensfunMakers()).toEqual(['Canon', 'Nikon']);
    expect(await lensfunMakers()).toEqual(['Canon', 'Nikon']);
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke).toHaveBeenCalledWith('get_lensfun_makers');
  });
});
