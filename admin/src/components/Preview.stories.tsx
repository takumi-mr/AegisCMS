import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import { Preview } from './Preview';

const meta = {
  title: 'Content/Preview',
  component: Preview,
  args: {
    contentType: 'article',
    title: 'A sample article',
    slug: 'sample-article',
    categoryId: '',
    blocks: [
      { name: 'core/heading', attributes: { level: 2 }, text: 'Preview heading' },
      { name: 'core/paragraph', attributes: {}, text: 'Preview content appears here.' },
    ],
  },
} satisfies Meta<typeof Preview>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Article: Story = {};
export const BlankPage: Story = {
  args: { contentType: 'page', layout: 'blank', title: 'Landing page' },
};

export const Fullscreen: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = canvas.getByRole('button', { name: 'Full screen' });
    await expect(toggle).toHaveAttribute('aria-pressed', 'false');
    await userEvent.click(toggle);
    await expect(canvas.getByRole('button', { name: 'Exit full screen' })).toHaveAttribute(
      'aria-pressed',
      'true'
    );
    await userEvent.keyboard('{Escape}');
    await expect(canvas.getByRole('button', { name: 'Full screen' })).toHaveAttribute(
      'aria-pressed',
      'false'
    );
    // No themed page has been rendered yet, so there is nothing to open in a new window
    await expect(canvas.getByRole('button', { name: 'Open in a separate window' })).toBeDisabled();
  },
};

export const PreviewAsAReader: Story = {
  args: { access: 'paid', memberLevels: ['free', 'paid'] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const select = canvas.getByRole('combobox', { name: 'Preview as' });
    await expect(select).toHaveValue('staff');
    await userEvent.selectOptions(select, 'guest');
    await expect(select).toHaveValue('guest');
    await userEvent.selectOptions(select, 'member:paid');
    await expect(select).toHaveValue('member:paid');
  },
};

/** Content everyone can read looks the same to every reader, so there is nothing to choose */
export const PublicContentHasNoReaderChoice: Story = {
  args: { access: '', memberLevels: ['free', 'paid'] },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole('combobox', { name: 'Preview as' })).toBeNull();
  },
};
