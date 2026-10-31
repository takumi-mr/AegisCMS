import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';

// The script and the style are served by the AegisCMS server (crates/aegis-server/assets). They are
// loaded as text here and run in the real browser, to test what visitors get.
import consentScript from '../../../crates/aegis-server/assets/consent.js?raw';
import consentStyle from '../../../crates/aegis-server/assets/consent.css?raw';

const REV = '1-test';
const CATEGORIES = ['analytics', 'marketing'];

type Win = Window & {
  AegisConsent?: {
    has(id: string): boolean;
    get(): Record<string, boolean> | null;
    open(): void;
    onChange(fn: (choices: Record<string, boolean>) => void): void;
    destroy(): void;
  };
  __ran?: string[];
};
const win = (): Win => window as Win;

const readCookie = (): { r: string; c: Record<string, boolean> } | null => {
  const part = document.cookie.split('; ').find((p) => p.startsWith('aegis_consent='));
  return part ? JSON.parse(decodeURIComponent(part.slice('aegis_consent='.length))) : null;
};

/**
 * The markup the server's built-in consent.html writes (a theme's own consent.html only has to keep the
 * data-aegis-consent-* attributes). The Rust tests check that the built-in template has all of them.
 */
const BANNER = (layout: string) => `
<div class="aegis-consent aegis-consent-layout-${layout} aegis-consent-pos-${
  layout === 'modal' ? 'center' : layout === 'box' ? 'bottom-right' : 'bottom'
}${layout === 'modal' ? ' aegis-consent-backdrop' : ''}" data-aegis-consent-root role="dialog"${
  layout === 'modal' ? ' aria-modal="true"' : ''
} aria-labelledby="aegis-consent-title" aria-describedby="aegis-consent-message" hidden>
  <div class="aegis-consent-body">
    <h2 class="aegis-consent-title" id="aegis-consent-title">Cookies</h2>
    <p class="aegis-consent-message" id="aegis-consent-message">We use cookies to make the site work. <a class="aegis-consent-policy" href="/privacy">Privacy policy</a></p>
    <div class="aegis-consent-details" data-aegis-consent-details hidden>
      <label class="aegis-consent-category"><input type="checkbox" checked disabled><span><strong>Necessary</strong><small>Always on.</small></span></label>
      <label class="aegis-consent-category"><input type="checkbox" data-aegis-consent-category="analytics"><span><strong>Analytics</strong><small>Helps us understand the site.</small></span></label>
      <label class="aegis-consent-category"><input type="checkbox" data-aegis-consent-category="marketing"><span><strong>Marketing</strong><small>Ads.</small></span></label>
    </div>
    <div class="aegis-consent-actions">
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="reject-all">Reject all</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="customize">Customize</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="save" hidden>Save choices</button>
      <button type="button" class="aegis-consent-button" data-aegis-consent-action="accept-all">Accept all</button>
    </div>
  </div>
</div>
<button type="button" class="aegis-consent-button aegis-consent-reopen" data-aegis-consent-reopen hidden>Cookie settings</button>`;

/** A theme's own, simpler banner: two buttons, no per-category choices */
const SIMPLE_BANNER = `
<aside data-aegis-consent-root hidden class="mine">
  <p>Cookies?</p>
  <button type="button" data-aegis-consent-action="reject-all">No</button>
  <button type="button" data-aegis-consent-action="accept-all">Yes</button>
</aside>`;

/** What a page of the site has: gated scripts and a frame that wait for consent, and a settings link */
const Page = ({ banner = BANNER('banner') }: { banner?: string }) => (
  <div>
    <style>{consentStyle}</style>
    <p>Page content</p>
    <a href="#settings" data-aegis-consent-open>
      Open cookie settings
    </a>
    <div
      dangerouslySetInnerHTML={{
        __html: `
          <script type="text/plain" data-aegis-consent="analytics">(window.__ran = window.__ran || []).push('analytics')</script>
          <script type="text/plain" data-aegis-consent="marketing">(window.__ran = window.__ran || []).push('marketing')</script>
          <iframe title="map" data-aegis-consent="marketing" data-src="about:blank#loaded"></iframe>
          ${banner}`,
      }}
    />
  </div>
);

/** Puts the script's settings in the page and runs the script, like the server's <head> does */
const start = (overrides: Record<string, unknown> = {}) => {
  const config = document.createElement('script');
  config.type = 'application/json';
  config.id = 'aegis-consent-config';
  config.textContent = JSON.stringify({
    rev: REV,
    days: 180,
    delay: 0,
    modal: false,
    categories: CATEGORIES,
    ...overrides,
  });
  document.head.appendChild(config);
  new Function(consentScript)();
};

const reset = () => {
  document.cookie = 'aegis_consent=; Max-Age=0; Path=/';
  win().AegisConsent?.destroy();
  delete win().AegisConsent;
  delete win().__ran;
  document.getElementById('aegis-consent-config')?.remove();
};

const meta = {
  title: 'Site/CookieConsent',
  component: Page,
  beforeEach: () => {
    reset();
    return reset;
  },
} satisfies Meta<typeof Page>;

export default meta;
type Story = StoryObj<typeof meta>;

const root = () => document.querySelector<HTMLElement>('[data-aegis-consent-root]')!;
const isOpen = () => !root().hidden;

/** Nothing was chosen: the banner asks, and nothing that needs consent runs */
export const AsksFirst: Story = {
  play: async () => {
    start();
    const banner = await within(document.body).findByRole('dialog');
    await expect(banner).toBeVisible();
    await expect(within(banner).getByRole('heading', { name: 'Cookies' })).toBeVisible();
    await expect(within(banner).getByRole('link', { name: 'Privacy policy' })).toHaveAttribute(
      'href',
      '/privacy'
    );
    await expect(banner).toHaveAttribute('data-aegis-consent-state', 'ask');
    // Refusing is as easy as accepting
    await expect(within(banner).getByRole('button', { name: 'Reject all' })).toBeVisible();
    await expect(within(banner).getByRole('button', { name: 'Accept all' })).toBeVisible();
    // The per-category choices are not shown yet
    await expect(document.querySelector('[data-aegis-consent-details]')).not.toBeVisible();
    await expect(win().__ran).toBeUndefined();
    await expect(win().AegisConsent?.get()).toBeNull();
    await expect(readCookie()).toBeNull();
    // The refusing button has the focus, so a reflex key press does not accept
    await expect(within(banner).getByRole('button', { name: 'Reject all' })).toHaveFocus();
  },
};

/** Accepting runs the waiting scripts and frames, saves the choice and announces it */
export const AcceptAll: Story = {
  play: async () => {
    const events = fn();
    document.addEventListener('aegis:consent', (e) => events((e as CustomEvent).detail), {
      once: true,
    });
    start();
    await userEvent.click(await within(document.body).findByRole('button', { name: 'Accept all' }));
    await waitFor(() => expect(win().__ran).toEqual(['analytics', 'marketing']));
    await expect(win().AegisConsent?.has('analytics')).toBe(true);
    await expect(win().AegisConsent?.get()).toEqual({ analytics: true, marketing: true });
    await expect(readCookie()).toEqual({ r: REV, c: { analytics: true, marketing: true } });
    await expect(events).toHaveBeenCalledWith({
      categories: { analytics: true, marketing: true },
    });
    // The frame got its address, and the banner is replaced by the small settings button
    await expect(document.querySelector('iframe')?.getAttribute('src')).toBe('about:blank#loaded');
    await expect(isOpen()).toBe(false);
    await expect(
      within(document.body).getByRole('button', { name: 'Cookie settings' })
    ).toBeVisible();
    // The waiting scripts were replaced by real ones, once
    await expect(document.querySelectorAll('script[type="text/plain"]')).toHaveLength(0);
  },
};

/** Rejecting saves the choice and runs nothing */
export const RejectAll: Story = {
  play: async () => {
    start();
    await userEvent.click(await within(document.body).findByRole('button', { name: 'Reject all' }));
    await expect(readCookie()).toEqual({ r: REV, c: { analytics: false, marketing: false } });
    await expect(win().__ran).toBeUndefined();
    await expect(win().AegisConsent?.has('marketing')).toBe(false);
    await expect(document.querySelector('iframe')?.getAttribute('src')).toBeNull();
    await expect(isOpen()).toBe(false);
  },
};

/** Choosing categories one by one */
export const Customize: Story = {
  play: async () => {
    start();
    await userEvent.click(await within(document.body).findByRole('button', { name: 'Customize' }));
    const banner = root();
    await expect(banner).toHaveAttribute('data-aegis-consent-state', 'details');
    // The choices appear and "Customize" gives way to "Save choices"
    await expect(within(banner).queryByRole('button', { name: 'Customize' })).toBeNull();
    // "Necessary" is always on and cannot be turned off
    const necessary = within(banner).getByRole('checkbox', { name: /Necessary/ });
    await expect(necessary).toBeChecked();
    await expect(necessary).toBeDisabled();
    await userEvent.click(within(banner).getByRole('checkbox', { name: /Analytics/ }));
    await userEvent.click(within(banner).getByRole('button', { name: 'Save choices' }));
    await waitFor(() => expect(win().__ran).toEqual(['analytics']));
    await expect(readCookie()?.c).toEqual({ analytics: true, marketing: false });
    await expect(document.querySelector('iframe')?.getAttribute('src')).toBeNull();
  },
};

/** A saved choice is applied right away, without asking again */
export const RemembersTheChoice: Story = {
  play: async () => {
    document.cookie = `aegis_consent=${encodeURIComponent(
      JSON.stringify({ r: REV, c: { analytics: true, marketing: false } })
    )}; Path=/`;
    start();
    await waitFor(() => expect(win().__ran).toEqual(['analytics']));
    await expect(isOpen()).toBe(false);
    await expect(win().AegisConsent?.has('analytics')).toBe(true);
    await expect(win().AegisConsent?.has('marketing')).toBe(false);
    // onChange tells a late listener what is known
    const late = fn();
    win().AegisConsent?.onChange(late);
    await expect(late).toHaveBeenCalledWith({ analytics: true, marketing: false });
  },
};

/** A new revision (the categories changed) asks again, and a broken cookie is ignored */
export const AsksAgainWhenTheRevisionChanges: Story = {
  play: async () => {
    document.cookie = `aegis_consent=${encodeURIComponent(
      JSON.stringify({ r: 'old', c: { analytics: true, marketing: true } })
    )}; Path=/`;
    start();
    await within(document.body).findByRole('dialog');
    await expect(win().__ran).toBeUndefined();
    await expect(win().AegisConsent?.get()).toBeNull();
  },
};

export const IgnoresABrokenCookie: Story = {
  play: async () => {
    document.cookie = 'aegis_consent=%7Bnot-json; Path=/';
    start();
    await within(document.body).findByRole('dialog');
    await expect(win().AegisConsent?.get()).toBeNull();
  },
};

/** A link with data-aegis-consent-open (or the small button) reopens the settings with the choices */
export const ReopensTheSettings: Story = {
  play: async ({ canvasElement }) => {
    document.cookie = `aegis_consent=${encodeURIComponent(
      JSON.stringify({ r: REV, c: { analytics: true, marketing: false } })
    )}; Path=/`;
    start();
    await waitFor(() => expect(win().__ran).toEqual(['analytics']));
    await userEvent.click(
      within(canvasElement).getByRole('link', { name: 'Open cookie settings' })
    );
    const banner = await within(document.body).findByRole('dialog');
    await expect(banner).toHaveAttribute('data-aegis-consent-state', 'details');
    await expect(within(banner).getByRole('checkbox', { name: /Analytics/ })).toBeChecked();
    await expect(within(banner).getByRole('checkbox', { name: /Marketing/ })).not.toBeChecked();
    // Escape closes it, because a choice was already made
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(isOpen()).toBe(false));
    // The small button opens it too
    await userEvent.click(within(document.body).getByRole('button', { name: 'Cookie settings' }));
    await within(document.body).findByRole('dialog');
    await expect(readCookie()?.c).toEqual({ analytics: true, marketing: false });
  },
};

/** Without a choice, Escape does not dismiss the question */
export const EscapeDoesNotSkipTheQuestion: Story = {
  play: async () => {
    start();
    await within(document.body).findByRole('dialog');
    await userEvent.keyboard('{Escape}');
    await expect(isOpen()).toBe(true);
    await expect(readCookie()).toBeNull();
  },
};

/** The question is shown a moment later when `delay_ms` is set */
export const WaitsBeforeAsking: Story = {
  play: async () => {
    start({ delay: 400 });
    await expect(isOpen()).toBe(false);
    await waitFor(() => expect(isOpen()).toBe(true), { timeout: 2000 });
  },
};

/** A modal keeps the keyboard focus inside the dialog */
export const ModalKeepsTheFocusInside: Story = {
  args: { banner: BANNER('modal') },
  play: async () => {
    start({ modal: true });
    const banner = await within(document.body).findByRole('dialog');
    await expect(banner).toHaveAttribute('aria-modal', 'true');
    const accept = within(banner).getByRole('button', { name: 'Accept all' });
    const reject = within(banner).getByRole('button', { name: 'Reject all' });
    await expect(reject).toHaveFocus();
    // Shift+Tab moves back to the policy link (the first control); from there it wraps to the last button,
    // and Tab from the last button wraps to the link again. The focus never leaves the dialog.
    const link = within(banner).getByRole('link', { name: 'Privacy policy' });
    await userEvent.tab({ shift: true });
    await expect(link).toHaveFocus();
    await userEvent.tab({ shift: true });
    await expect(accept).toHaveFocus();
    await userEvent.tab();
    await expect(link).toHaveFocus();
    await userEvent.tab();
    await expect(reject).toHaveFocus();
  },
};

/** The corner card layout works the same */
export const BoxLayout: Story = {
  args: { banner: BANNER('box') },
  play: async () => {
    start();
    const banner = await within(document.body).findByRole('dialog');
    await expect(banner).toHaveClass('aegis-consent-layout-box', 'aegis-consent-pos-bottom-right');
    await userEvent.click(within(banner).getByRole('button', { name: 'Accept all' }));
    await waitFor(() => expect(win().__ran).toEqual(['analytics', 'marketing']));
  },
};

/** A theme's own banner needs only the data attributes: two buttons, no per-category choices */
export const ThemeWithTwoButtons: Story = {
  args: { banner: SIMPLE_BANNER },
  play: async () => {
    start();
    const banner = await within(document.body).findByRole('complementary');
    await expect(banner).toBeVisible();
    await userEvent.click(within(banner).getByRole('button', { name: 'Yes' }));
    await waitFor(() => expect(win().__ran).toEqual(['analytics', 'marketing']));
    await expect(isOpen()).toBe(false);
    // There is no settings button in this theme, but the API still opens the banner
    win().AegisConsent?.open();
    await waitFor(() => expect(isOpen()).toBe(true));
  },
};

/** A page whose theme has no banner at all: nothing asks, and what needs consent never runs */
export const WithoutABannerNothingRuns: Story = {
  args: { banner: '' },
  play: async () => {
    const warn = fn();
    const original = console.warn;
    console.warn = warn;
    try {
      start();
      await expect(warn).toHaveBeenCalled();
    } finally {
      console.warn = original;
    }
    await expect(win().__ran).toBeUndefined();
    await expect(document.querySelector('iframe')?.getAttribute('src')).toBeNull();
    await expect(win().AegisConsent?.get()).toBeNull();
  },
};

/** A page without the settings element does nothing (the script is safe to load anywhere) */
export const DoesNothingWithoutSettings: Story = {
  play: async () => {
    new Function(consentScript)();
    await expect(win().AegisConsent).toBeUndefined();
    await expect(isOpen()).toBe(false);
  },
};
