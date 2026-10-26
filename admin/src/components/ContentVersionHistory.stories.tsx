import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { ContentVersionHistory } from './ContentVersionHistory';

const meta = {
  title: 'Content/ContentVersionHistory',
  component: ContentVersionHistory,
  args: {
    versions: [
      {
        revision: 3,
        created_at: '2026-10-26T04:00:00Z',
        created_by: 'admin',
        change_note: 'Restored revision 1',
      },
      { revision: 2, created_at: '2026-10-25T12:30:00Z', created_by: 'editor' },
      { revision: 1, created_at: '2026-10-24T09:00:00Z', created_by: 'admin' },
    ],
    onRestore: fn(),
  },
} satisfies Meta<typeof ContentVersionHistory>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Populated: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('heading', { name: /version history|版履歴/i })).toBeVisible();
    await expect(canvas.getByText(/Version 3|版 3/)).toBeVisible();
    await userEvent.click(canvas.getByRole('button', { name: /restore version 2|版 2 を復元/i }));
    await expect(args.onRestore).toHaveBeenCalledWith(2);
  },
};

export const Empty: Story = {
  args: { versions: [] },
};

export const Loading: Story = {
  args: { versions: [], loading: true },
};
