import { renderToStaticMarkup } from 'react-dom/server';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { initReactI18next } from 'react-i18next';
import i18n from 'i18next';
import type { ProviderLooks as Offer } from '../ag.generated';
import { AG_NS } from '../locales';
import en from '../locales/en.json';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('react-toastify', () => ({ toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() } }));

const { default: ProviderLooks, LookStrip } = await import('./ProviderLooks');
const { useProviderLooks } = await import('./providerLooksStore');
const { useLibraryStore } = await import('../../store/useLibraryStore');

const OPEN = 'D:/photos/open.cr3';

const OFFER: Offer = {
  cameras: [
    { id: 'cam1', name: 'Camera One', make: 'Acme', model: 'One' },
    { id: 'cam2', name: 'Camera Two', make: 'Acme', model: 'Two' },
  ],
  looks: [
    { id: 'look1', name: 'Invented Stock', group: null },
    { id: 'look2', name: 'Other Stock', group: null },
  ],
  options: [],
  on: true,
  photo: { camera: 'cam1', files: [], previews: [], beside: 0 },
};

/** A server render reads the store's first state. */
function offered(state: Partial<ReturnType<typeof useProviderLooks.getState>>) {
  Object.assign(useProviderLooks.getInitialState(), state);
  useProviderLooks.setState(state);
}

const section = () => renderToStaticMarkup(<ProviderLooks photo={OPEN} ready onTakeOff={() => {}} />);

beforeAll(async () => {
  await i18n.use(initReactI18next).init({ lng: 'en', resources: { en: { [AG_NS]: en } } });
});

beforeEach(() => {
  offered({
    offer: null,
    chosen: {},
    chosenOptions: {},
    waiting: {},
    note: null,
    batch: null,
    previewing: null,
    deleting: false,
  });
});

describe('with the setting absent or empty', () => {
  it('nothing shows', () => {
    expect(section()).toBe('');
  });
});

describe('a photo that shows no provider looks', () => {
  it('has no section and no switch', () => {
    offered({ offer: { ...OFFER, photo: null } });
    expect(section()).toBe('');
  });
});

describe('with the switch on', () => {
  beforeEach(() => offered({ offer: OFFER }));

  it('shows the header with its switch, the camera and the looks', () => {
    const html = section();
    expect(html).toContain('Provider looks');
    expect(html).toContain('role="switch"');
    expect(html).toContain('aria-checked="true"');
    expect(html).toContain('Presets and LUTs are off while this is on.');
    expect(html).toContain('Camera One');
    expect(html).toContain('Camera Two');
    expect(html).toContain('Matched to this photo');
    expect(html).toContain('Invented Stock');
    expect(html).toContain('Other Stock');
    expect(html).not.toContain('agm-look-dim');
  });

  it('says when the camera is not the photo’s', () => {
    offered({ chosen: { [OPEN]: 'cam2' } });
    expect(section()).toContain('Not this photo’s camera');
  });

  it('shows the spinner only once a look is taking a while', () => {
    const waited = { look: 'look1', options: {}, name: 'Invented Stock', job: 1, then: 'put', batch: false } as const;
    offered({ waiting: { [OPEN]: { ...waited, spinning: false } } });
    expect(section()).not.toContain('agm-spin');
    offered({ waiting: { [OPEN]: { ...waited, spinning: true } } });
    expect(section()).toContain('agm-spin');
  });

  it('says on the row why a look could not be made', () => {
    offered({ note: { photo: OPEN, look: 'look2', text: 'looksProviderFailed' } });
    expect(section()).toContain('Couldn’t make this look');
  });

  it('counts a batch, with Cancel', () => {
    offered({ batch: { name: 'Invented Stock', job: 1, done: 12, ended: 12, total: 40, before: [] } });
    const html = section();
    expect(html).toContain('Provider look: 12 of 40');
    expect(html).toContain('Cancel');
  });
});

describe('Preview all', () => {
  const button = (html: string) => html.match(/<button[^>]*>[^<]*Preview[^<]*<\/button>/)?.[0] ?? '';

  it('shows after the options and before the looks, ready to press', () => {
    offered({ offer: { ...OFFER, options: [{ id: 'optionA', label: 'Invented choice', default: false }] } });
    const html = section();
    expect(button(html)).toContain('Preview all');
    expect(button(html)).not.toContain('disabled');
    expect(html.indexOf('Invented choice')).toBeLessThan(html.indexOf('Preview all'));
    expect(html.indexOf('Preview all')).toBeLessThan(html.indexOf('Invented Stock'));
  });

  it('is dimmed and disabled with the switch off', () => {
    offered({ offer: { ...OFFER, on: false } });
    const html = section();
    expect(button(html)).toContain('disabled');
    expect(html).toContain('agm-provider-tools agm-look-dim');
  });

  it('waits for the photo', () => {
    offered({ offer: OFFER });
    const html = renderToStaticMarkup(<ProviderLooks photo={OPEN} ready={false} onTakeOff={() => {}} />);
    expect(button(html)).toContain('disabled');
  });

  it('counts on the button while it runs, and cannot be pressed again', () => {
    offered({ offer: OFFER, previewing: { run: 1, photo: OPEN, job: 1, ended: 3, total: 10 } });
    const html = section();
    expect(button(html)).toContain('Previewing 3 of 10');
    expect(button(html)).toContain('disabled');
  });

  it('has no place in the grid, where there is no one photo', () => {
    Object.assign(useLibraryStore.getInitialState(), { libraryActivePath: OPEN });
    offered({ offer: OFFER });
    const html = renderToStaticMarkup(<ProviderLooks photo={null} ready={false} onTakeOff={() => {}} />);
    Object.assign(useLibraryStore.getInitialState(), { libraryActivePath: null });
    expect(html).toContain('Invented Stock');
    expect(html).not.toContain('Preview all');
  });
});

describe('Delete all previews', () => {
  const WITH_FILES: Offer = { ...OFFER, photo: { camera: 'cam1', files: [], previews: [], beside: 2 } };
  const button = (html: string) => html.match(/<button[^>]*>[^<]*Delete[^<]*<\/button>/)?.[0] ?? '';

  it('shows beside Preview all, ready to press', () => {
    offered({ offer: WITH_FILES });
    const html = section();
    expect(button(html)).toContain('Delete all previews');
    expect(button(html)).not.toContain('disabled');
    expect(html.indexOf('Preview all')).toBeLessThan(html.indexOf('Delete all previews'));
    expect(html.indexOf('Delete all previews')).toBeLessThan(html.indexOf('Invented Stock'));
  });

  it('is disabled with no look file beside the photo', () => {
    offered({ offer: OFFER });
    expect(button(section())).toContain('disabled');
  });

  it('counts a look file the current list does not show', () => {
    offered({ offer: WITH_FILES });
    expect(WITH_FILES.photo?.files).toEqual([]);
    expect(button(section())).not.toContain('disabled');
  });

  it('is disabled with the switch off', () => {
    offered({ offer: { ...WITH_FILES, on: false } });
    const html = section();
    expect(button(html)).toContain('disabled');
    expect(html).toContain('agm-provider-tools agm-look-dim');
  });

  it('waits for the photo', () => {
    offered({ offer: WITH_FILES });
    const html = renderToStaticMarkup(<ProviderLooks photo={OPEN} ready={false} onTakeOff={() => {}} />);
    expect(button(html)).toContain('disabled');
  });

  it('cannot be pressed again while it runs', () => {
    offered({ offer: WITH_FILES, deleting: true });
    expect(button(section())).toContain('disabled');
  });

  it('can stop a running Preview all', () => {
    offered({ offer: WITH_FILES, previewing: { run: 1, photo: OPEN, job: 1, ended: 3, total: 10 } });
    expect(button(section())).not.toContain('disabled');
  });

  it('has no place in the grid, where there is no one photo', () => {
    Object.assign(useLibraryStore.getInitialState(), { libraryActivePath: OPEN });
    offered({ offer: WITH_FILES });
    const html = renderToStaticMarkup(<ProviderLooks photo={null} ready={false} onTakeOff={() => {}} />);
    Object.assign(useLibraryStore.getInitialState(), { libraryActivePath: null });
    expect(html).toContain('Invented Stock');
    expect(html).not.toContain('Delete all previews');
  });

  it('shows nothing with no provider', () => {
    offered({ offer: null });
    expect(section()).toBe('');
  });
});

describe('with the switch off', () => {
  it('dims the looks and the camera and says what the switch does', () => {
    offered({ offer: { ...OFFER, on: false } });
    const html = section();
    expect(html).toContain('aria-checked="false"');
    expect(html).toContain('Turn on to use these. Presets and LUTs go off.');
    expect(html).toContain('agm-look-dim');
    expect(html).toContain('agm-provider-camera agm-look-dim');
  });
});

describe('an option the provider offers', () => {
  const OPTION = { id: 'optionA', label: 'Invented <choice>', default: false };
  const checkboxes = (html: string) => html.match(/<input[^>]*type="checkbox"[^>]*>/g) ?? [];

  it('with none offered, no checkbox shows', () => {
    offered({ offer: OFFER });
    expect(checkboxes(section())).toEqual([]);
  });

  it('shows as a checkbox with the provider’s label, as text', () => {
    offered({ offer: { ...OFFER, options: [OPTION] } });
    const html = section();
    expect(checkboxes(html)).toHaveLength(1);
    expect(html).toContain('Invented &lt;choice&gt;');
    expect(html).not.toContain('Invented <choice>');
  });

  it('starts unchecked when its default is off', () => {
    offered({ offer: { ...OFFER, options: [OPTION] } });
    expect(checkboxes(section())[0]).not.toContain('checked');
  });

  it('starts checked when its default is on', () => {
    offered({ offer: { ...OFFER, options: [{ ...OPTION, default: true }] } });
    expect(checkboxes(section())[0]).toContain('checked');
  });

  it('shows the hand choice for this photo', () => {
    offered({ offer: { ...OFFER, options: [OPTION] }, chosenOptions: { [OPEN]: { optionA: true } } });
    expect(checkboxes(section())[0]).toContain('checked');
  });

  it('is dimmed and disabled with the switch off', () => {
    offered({ offer: { ...OFFER, on: false, options: [OPTION] } });
    const html = section();
    expect(checkboxes(html)[0]).toContain('disabled');
    expect(html).toContain('agm-provider-option agm-look-dim');
  });

  it('is not dimmed with the switch on', () => {
    offered({ offer: { ...OFFER, options: [OPTION] } });
    const html = section();
    expect(checkboxes(html)[0]).not.toContain('disabled');
    expect(html).not.toContain('agm-look-dim');
  });

  it('sits after the camera and before the looks', () => {
    offered({ offer: { ...OFFER, options: [OPTION] } });
    const html = section();
    expect(html.indexOf('Matched to this photo')).toBeLessThan(html.indexOf('Invented &lt;choice&gt;'));
    expect(html.indexOf('Invented &lt;choice&gt;')).toBeLessThan(html.indexOf('Invented Stock'));
  });
});

describe('the look on the photo', () => {
  const strip = (there: boolean, canMake: boolean) =>
    renderToStaticMarkup(
      <LookStrip
        canMake={canMake}
        kind="Camera One"
        name="Stock 1.5"
        onMake={() => {}}
        onStrength={() => {}}
        onTakeOff={() => {}}
        strength={100}
        swatch={undefined}
        there={there}
      />,
    );

  it('keeps its whole name, its camera, its Strength and Remake', () => {
    const html = strip(true, true);
    expect(html).toContain('Stock 1.5');
    expect(html).toContain('Camera One');
    expect(html).toContain('Strength');
    expect(html).toContain('Remake');
    expect(html).not.toContain('Look file missing');
  });

  it('says when its file is missing, and offers to make it again', () => {
    const html = strip(false, true);
    expect(html).toContain('Look file missing. Showing Linear without the look.');
    expect(html).toContain('Make it again');
    expect(html).not.toContain('Strength');
  });

  it('offers neither with no provider', () => {
    const html = strip(false, false);
    expect(html).toContain('Look file missing');
    expect(html).not.toContain('Make it again');
    expect(html).not.toContain('Remake');
  });
});
