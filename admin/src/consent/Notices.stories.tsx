import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';

// The script and the style are served by the AegisCMS server (crates/aegis-server/assets). They are
// loaded as text here and run in the real browser, to test what visitors get.
import noticesScript from '../../../crates/aegis-server/assets/notices.js?raw';
import noticesStyle from '../../../crates/aegis-server/assets/notices.css?raw';

const KEY = 'aegis_notices';

/** The markup the server's built-in notices.html writes (a theme's own only has to keep the data attributes) */
const NOTICES = `
<div class="aegis-notices" role="region" aria-label="Notices">
  <div class="aegis-notice aegis-notice-warning" data-aegis-notice="n1" data-aegis-notice-rev="r1" role="alert">
    <p class="aegis-notice-message">Maintenance tonight. <a class="aegis-notice-link" href="/status">Learn more</a></p>
    <button type="button" class="aegis-notice-close" data-aegis-notice-close hidden aria-label="Close this notice">&times;</button>
  </div>
  <div class="aegis-notice aegis-notice-info" data-aegis-notice="n2" data-aegis-notice-rev="r2" role="status">
    <p class="aegis-notice-message">Sale!</p>
  </div>
</div>`;

const Page = ({ markup = NOTICES }: { markup?: string }) => (
  <div>
    <style>{noticesStyle}</style>
    <div dangerouslySetInnerHTML={{ __html: markup }} />
    <p>Page content</p>
  </div>
);

const run = () => new Function(noticesScript)();
const remembered = (): Record<string, string> =>
  JSON.parse(localStorage.getItem(KEY) || '{}') as Record<string, string>;

const meta = {
  title: 'Site/Notices',
  component: Page,
  beforeEach: () => {
    localStorage.removeItem(KEY);
    return () => localStorage.removeItem(KEY);
  },
} satisfies Meta<typeof Page>;

export default meta;
type Story = StoryObj<typeof meta>;

/** Only the notices that can be closed get a close button; the others stay */
export const ShowsCloseButtonsOnlyWhereTheyBelong: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    // Without the script, there is no (useless) button to see
    await expect(canvas.getByText('Maintenance tonight.', { exact: false })).toBeVisible();
    await expect(canvas.queryByRole('button', { name: 'Close this notice' })).toBeNull();
    run();
    await expect(canvas.getByRole('button', { name: 'Close this notice' })).toBeVisible();
    await expect(canvas.getAllByRole('button')).toHaveLength(1);
  },
};

/** Closing removes the notice, remembers it, and the empty wrapper goes with the last one */
export const ClosingRemembersTheNotice: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    run();
    await userEvent.click(canvas.getByRole('button', { name: 'Close this notice' }));
    await expect(canvas.queryByText('Maintenance tonight.', { exact: false })).toBeNull();
    // The one that cannot be closed stays, and the wrapper with it
    await expect(canvas.getByText('Sale!')).toBeVisible();
    await expect(remembered()).toEqual({ n1: 'r1' });
  },
};

/** A notice closed earlier stays gone on the next page, until it is edited (a new version) */
export const AClosedNoticeStaysClosedUntilItChanges: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    localStorage.setItem(KEY, JSON.stringify({ n1: 'r1' }));
    run();
    await expect(canvas.queryByText('Maintenance tonight.', { exact: false })).toBeNull();
    await expect(canvas.getByText('Sale!')).toBeVisible();
  },
};

export const ComesBackWhenEdited: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    localStorage.setItem(KEY, JSON.stringify({ n1: 'an-older-version' }));
    run();
    await expect(canvas.getByText('Maintenance tonight.', { exact: false })).toBeVisible();
    await expect(canvas.getByRole('button', { name: 'Close this notice' })).toBeVisible();
  },
};

/** The wrapper is removed when its last notice is closed */
export const TheWrapperGoesWithTheLastNotice: Story = {
  args: {
    markup: `<div class="aegis-notices"><div data-aegis-notice="only" data-aegis-notice-rev="1">
      <p>Only one</p><button type="button" data-aegis-notice-close hidden>close</button></div></div>`,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    run();
    await userEvent.click(canvas.getByRole('button', { name: 'close' }));
    await expect(canvasElement.querySelector('.aegis-notices')).toBeNull();
  },
};

/** Blocked storage (private modes) cannot remember, but closing still works for the page view */
export const WorksWhenStorageIsBlocked: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = () => {
      throw new Error('blocked');
    };
    try {
      run();
      await userEvent.click(canvas.getByRole('button', { name: 'Close this notice' }));
      await expect(canvas.queryByText('Maintenance tonight.', { exact: false })).toBeNull();
    } finally {
      Storage.prototype.setItem = original;
    }
  },
};

/** A page without notices: the script does nothing */
export const DoesNothingWithoutNotices: Story = {
  args: { markup: '' },
  play: async ({ canvasElement }) => {
    run();
    await expect(canvasElement.querySelector('[data-aegis-notice]')).toBeNull();
    await expect(localStorage.getItem(KEY)).toBeNull();
  },
};

/** Each kind has a look of its own */
export const KindsHaveTheirOwnColors: Story = {
  args: {
    markup: ['info', 'success', 'warning', 'alert']
      .map(
        (style) =>
          `<div class="aegis-notice aegis-notice-${style}" data-aegis-notice="${style}"><p class="aegis-notice-message">${style}</p></div>`
      )
      .join(''),
  },
  play: async ({ canvasElement }) => {
    const colors = ['info', 'success', 'warning', 'alert'].map(
      (style) =>
        getComputedStyle(canvasElement.querySelector(`.aegis-notice-${style}`)!).backgroundColor
    );
    await expect(new Set(colors).size).toBe(4);
  },
};
