import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { ContentVersionPreviewModal } from './ContentVersionPreviewModal';

const meta = {
  title: 'Content/ContentVersionPreviewModal',
  component: ContentVersionPreviewModal,
  args: {
    contentType: 'article',
    version: {
      revision: 2,
      created_at: '2026-10-25T12:30:00Z',
      created_by: 'editor',
      title: 'Old title',
      slug: 'sample',
      blocks: [
        { name: 'core/heading', attributes: { level: 2 }, text: 'Heading' },
        { name: 'core/paragraph', attributes: {}, text: 'The old paragraph.' },
        { name: 'core/paragraph', attributes: {}, text: 'Unchanged paragraph.' },
      ],
      fields: { excerpt: 'Old excerpt' },
    },
    current: {
      title: 'New title',
      slug: 'sample',
      blocks: [
        { name: 'core/heading', attributes: { level: 2 }, text: 'Heading' },
        { name: 'core/paragraph', attributes: {}, text: 'The new paragraph.' },
        { name: 'core/paragraph', attributes: {}, text: 'Unchanged paragraph.' },
      ],
      fields: { excerpt: 'New excerpt' },
    },
    onRestore: fn(),
    onClose: fn(),
  },
} satisfies Meta<typeof ContentVersionPreviewModal>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement, args }) => {
    const screen = within(canvasElement.ownerDocument.body);
    await expect(
      await screen.findByRole('tab', { name: /^(Preview|プレビュー)$/ })
    ).toHaveAttribute('aria-selected', 'true');
    await userEvent.click(screen.getByRole('button', { name: /restore version 2|版 2 を復元/i }));
    await expect(args.onRestore).toHaveBeenCalledWith(2);
  },
};

export const Changes: Story = {
  play: async ({ canvasElement }) => {
    const screen = within(canvasElement.ownerDocument.body);
    await userEvent.click(await screen.findByRole('tab', { name: /^(Changes|差分)$/ }));
    await expect(screen.getByText('The old paragraph.')).toBeVisible();
    await expect(screen.getByText('The new paragraph.')).toBeVisible();
    await expect(screen.getByText('Old excerpt')).toBeVisible();
    // The unchanged paragraph is not reported as a change (it is only context in the body diff)
    await expect(screen.queryByText('Old title')).toBeVisible();
  },
};

export const NoDifference: Story = {
  args: {
    current: {
      title: 'Old title',
      slug: 'sample',
      blocks: [
        { name: 'core/heading', attributes: { level: 2 }, text: 'Heading' },
        { name: 'core/paragraph', attributes: {}, text: 'The old paragraph.' },
        { name: 'core/paragraph', attributes: {}, text: 'Unchanged paragraph.' },
      ],
      fields: { excerpt: 'Old excerpt' },
    },
  },
  play: async ({ canvasElement }) => {
    const screen = within(canvasElement.ownerDocument.body);
    await userEvent.click(await screen.findByRole('tab', { name: /^(Changes|差分)$/ }));
    await expect(screen.getByRole('status')).toBeVisible();
  },
};
