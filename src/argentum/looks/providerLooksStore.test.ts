import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LookProgress, ProviderLook, ProviderLooks } from '../ag.generated';

/** Every `ag` call, in order. */
const calls: { name: string; args: unknown }[] = [];
const invoked: { name: string; args: unknown }[] = [];
let answerOffer: ProviderLooks | null = null;
let hear: ((event: { payload: LookProgress }) => void) | null = null;
const listened = vi.fn();
const sidecars: Record<string, Record<string, unknown>> = {};

vi.mock('../ag', () => ({
  ag: vi.fn(async (name: string, args: unknown) => {
    calls.push({ name, args });
    if (name === 'provider_looks') return answerOffer;
    if (name === 'provider_look_make') return 7;
    return null;
  }),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (name: string, args: { path?: string }) => {
    invoked.push({ name, args });
    if (name === 'load_metadata') return { adjustments: sidecars[args.path ?? ''] ?? {} };
    return null;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: LookProgress }) => void) => {
    listened();
    hear = handler;
    return () => {};
  }),
}));
vi.mock('react-toastify', () => ({ toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() } }));
vi.mock('../locales', () => ({ agText: (key: string) => key }));

const { useProviderLooks, regularLooksAreOff, takeFromPaste, lookFor, restoreLook } =
  await import('./providerLooksStore');
const { useEditorStore } = await import('../../store/useEditorStore');
const { PROVIDER_LOOK_PATH } = await import('./providerLook');

const OFFER: ProviderLooks = {
  cameras: [
    { id: 'cam1', name: 'Camera One', make: 'Acme', model: 'One' },
    { id: 'cam2', name: 'Camera Two', make: 'Acme', model: 'Two' },
  ],
  looks: [
    { id: 'look1', name: 'Invented Stock', group: null },
    { id: 'look2', name: 'Other Stock', group: null },
  ],
  on: true,
  photo: { camera: 'cam1', files: [] },
};

const LOOK = OFFER.looks[0];
const OPEN = 'D:/photos/open.cr3';
const OTHER = 'D:/photos/other.cr3';

const progress = (path: string, outcome: LookProgress['outcome'], look = 'look1'): LookProgress => ({
  jobId: 7,
  path,
  camera: 'cam1',
  look,
  outcome,
});

/** Lets the promises in flight settle. */
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

const namesCalled = () => calls.map((c) => c.name);

async function offered(offer: ProviderLooks | null = OFFER) {
  answerOffer = offer;
  await useProviderLooks.getState().load(OPEN);
}

function openPhoto(adjustments: Record<string, unknown> = {}) {
  useEditorStore.setState({
    selectedImage: { path: OPEN } as never,
    adjustments: { rawToneRendering: 'dngStandard', ...adjustments } as never,
  });
}

beforeEach(() => {
  calls.length = 0;
  invoked.length = 0;
  listened.mockClear();
  useProviderLooks.getState().reset();
  openPhoto();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('with no provider', () => {
  it('nothing is offered and nothing listens', async () => {
    await offered(null);
    expect(useProviderLooks.getState().offer).toBeNull();
    expect(listened).not.toHaveBeenCalled();
  });

  it('regular looks stay on', async () => {
    await offered(null);
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('a paste is left as it is', async () => {
    await offered(null);
    const changes = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };
    expect(takeFromPaste([OTHER], changes)).toBe(changes);
    expect(namesCalled()).not.toContain('provider_look_make');
  });
});

describe('the switch', () => {
  it('turns regular looks off for a photo that shows provider looks', async () => {
    await offered();
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(true);
  });

  it('leaves them on while it is off', async () => {
    await offered({ ...OFFER, on: false });
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('leaves them on for a photo with no provider looks, even when on', async () => {
    await offered({ ...OFFER, photo: null });
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('is saved, and cancels what is waiting', async () => {
    await offered();
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    await useProviderLooks.getState().setOn(false);
    expect(calls).toContainEqual({ name: 'provider_looks_switch', args: { on: false } });
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
    expect(useProviderLooks.getState().offer?.on).toBe(false);
  });

  it('never touches a look already on the photo', async () => {
    const on = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };
    openPhoto(on);
    await offered();
    await useProviderLooks.getState().setOn(false);
    expect(useEditorStore.getState().adjustments.lutPath).toBe(PROVIDER_LOOK_PATH);
  });
});

describe('picking a look', () => {
  beforeEach(() => offered());

  it('asks for the file and leaves the photo as it is', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OPEN], camera: 'cam1', look: 'look1', remake: false, front: true },
    });
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('shows the spinner only after 300 ms', async () => {
    vi.useFakeTimers();
    const asking = useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    await vi.advanceTimersByTimeAsync(299);
    await asking;
    expect(useProviderLooks.getState().waiting[OPEN].spinning).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    expect(useProviderLooks.getState().waiting[OPEN].spinning).toBe(true);
  });

  it('puts Linear and the LUT on in one step when the file is ready', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    const { adjustments } = useEditorStore.getState();
    expect(adjustments).toMatchObject({
      lutPath: PROVIDER_LOOK_PATH,
      lutName: 'Invented Stock',
      lutSize: 64,
      lutIntensity: 100,
      lutIsSceneReferred: false,
      agLutDeclaredInput: true,
      rawToneRendering: 'linear',
      rawToneCurve: null,
      rawToneCurveMode: null,
      agProviderLook: {
        camera: 'cam1',
        look: 'look1',
        before: { rawToneRendering: 'dngStandard', rawToneCurve: null, rawToneCurveMode: null },
      },
    });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });

  it('goes on another photo through its own file if that one was left', async () => {
    sidecars[OTHER] = { rawToneRendering: 'baseCurve' };
    await useProviderLooks.getState().pick(OTHER, LOOK, 'cam1');
    hear?.({ payload: progress(OTHER, 'done') });
    await settled();
    const put = invoked.find((c) => c.name === 'apply_adjustments_to_paths')?.args as {
      paths: string[];
      adjustments: { agProviderLook: ProviderLook };
    };
    expect(put.paths).toEqual([OTHER]);
    expect(put.adjustments.agProviderLook.before.rawToneRendering).toBe('baseCurve');
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('says so on the row when it could not be made, and the photo stays', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    hear?.({ payload: progress(OPEN, 'failed') });
    await settled();
    expect(useProviderLooks.getState().note).toEqual({ photo: OPEN, look: 'look1', text: 'looksProviderFailed' });
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('says a folder cannot be written', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    hear?.({ payload: progress(OPEN, 'folder') });
    await settled();
    expect(useProviderLooks.getState().note?.text).toBe('looksProviderFolder');
  });

  it('the latest pick wins: the one it replaced reports nothing', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    await useProviderLooks.getState().pick(OPEN, OFFER.looks[1], 'cam1');
    hear?.({ payload: progress(OPEN, 'cancelled') });
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
    expect(useProviderLooks.getState().waiting[OPEN]?.look).toBe('look2');
  });

  it('Esc or a regular LUT cancels it', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    await useProviderLooks.getState().cancel(OPEN);
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });
});

describe('the camera picker', () => {
  beforeEach(() => offered());

  it('starts on the photo’s own camera', () => {
    expect(useProviderLooks.getState().cameraOf(OPEN, {})).toBe('cam1');
  });

  it('starts on the camera the edit names', () => {
    const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam2', look: 'look1', before: {} } };
    expect(useProviderLooks.getState().cameraOf(OPEN, edit)).toBe('cam2');
  });

  it('keeps a hand choice for that photo only', async () => {
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(useProviderLooks.getState().cameraOf(OPEN, {})).toBe('cam2');
    expect(useProviderLooks.getState().cameraOf(OTHER, {})).toBe('cam1');
  });

  it('cancels a look still being made', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1');
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });

  it('with no look on only changes what a click makes', async () => {
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('with a look on makes the same look for the new camera', async () => {
    const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look2', before: {} } };
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', edit);
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OPEN], camera: 'cam2', look: 'look2', remake: false, front: true },
    });
  });
});

describe('a batch', () => {
  beforeEach(() => offered());

  it('counts as each photo finishes, and its Undo removes what went on', async () => {
    sidecars[OTHER] = { rawToneRendering: 'baseCurve' };
    await useProviderLooks.getState().putOnSelection([OTHER, 'D:/photos/third.cr3'], LOOK, null);
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OTHER, 'D:/photos/third.cr3'], camera: null, look: 'look1', remake: false, front: false },
    });
    hear?.({ payload: progress(OTHER, 'done') });
    await settled();
    expect(useProviderLooks.getState().batch).toMatchObject({ done: 1, total: 2 });

    await useProviderLooks.getState().cancelBatch();
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().batch).toBeNull();

    invoked.length = 0;
    await useProviderLooks.getState().undoLast();
    expect(invoked.find((c) => c.name === 'apply_adjustments_to_paths')?.args).toMatchObject({
      paths: [OTHER],
      adjustments: { rawToneRendering: 'baseCurve', lutPath: null },
    });
  });
});

describe('a batch with the photo the app has open', () => {
  beforeEach(() => offered());

  it('is undone there too', async () => {
    await useProviderLooks.getState().putOnSelection([OPEN], LOOK, null);
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments.lutPath).toBe(PROVIDER_LOOK_PATH);

    await useProviderLooks.getState().undoLast();
    const { adjustments } = useEditorStore.getState();
    expect(adjustments.lutPath ?? null).toBeNull();
    expect(adjustments.rawToneRendering).toBe('dngStandard');
    expect(adjustments.agProviderLook ?? null).toBeNull();
  });
});

describe('a paste', () => {
  const pasted = {
    exposure: 1,
    lutPath: PROVIDER_LOOK_PATH,
    lutName: 'Invented Stock',
    lutIntensity: 100,
    rawToneRendering: 'linear',
    agProviderLook: { camera: 'cam2', look: 'look1', before: { rawToneRendering: 'default' } },
  };

  beforeEach(() => offered());

  it('sends the rest now and queues each photo’s own file, keeping the chosen camera', async () => {
    const rest = takeFromPaste([OTHER], pasted);
    expect(rest).toEqual({ exposure: 1 });
    await settled();
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OTHER], camera: 'cam2', look: 'look1', remake: false, front: false },
    });
  });

  it('of a look the provider does not list leaves it out', async () => {
    const unknown = { ...pasted, agProviderLook: { camera: 'cam2', look: 'gone', before: {} } };
    expect(takeFromPaste([OTHER], unknown)).toEqual({ exposure: 1 });
    await settled();
    expect(namesCalled()).not.toContain('provider_look_make');
  });
});

describe('opening a photo with a look whose file is missing', () => {
  const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };

  beforeEach(() => offered());

  it('asks for the file once', async () => {
    await restoreLook(OPEN, edit);
    await restoreLook(OPEN, edit);
    expect(calls.filter((c) => c.name === 'provider_look_make')).toHaveLength(1);
  });

  it('asks for nothing when the file is there', async () => {
    await offered({ ...OFFER, photo: { camera: 'cam1', files: [{ camera: 'cam1', look: 'look1', path: 'x' }] } });
    await restoreLook(OPEN, edit);
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('asks for nothing when the camera or look is not listed', async () => {
    await restoreLook(OPEN, { ...edit, agProviderLook: { camera: 'gone', look: 'look1', before: {} } });
    await restoreLook(OPEN, { ...edit, agProviderLook: { camera: 'cam1', look: 'gone', before: {} } });
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('leaves the edit alone when the file arrives', async () => {
    await restoreLook(OPEN, edit);
    openPhoto(edit);
    const before = useEditorStore.getState().adjustments;
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments).toEqual(before);
    expect(useEditorStore.getState().adjustments).not.toBe(before);
  });
});

describe('lookFor', () => {
  it('reads the look on an edit only while the marker is on', () => {
    const look = { camera: 'cam1', look: 'look1', before: {} };
    expect(lookFor({ lutPath: PROVIDER_LOOK_PATH, agProviderLook: look })).toEqual(look);
    expect(lookFor({ lutPath: 'film.cube', agProviderLook: look })).toBeNull();
  });
});
