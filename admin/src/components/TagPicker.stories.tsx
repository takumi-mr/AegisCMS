import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { Tag } from '../types';
import { TagPicker } from './TagPicker';

const tag = (id: string, name: string): Tag => ({
  id,
  slug: name.toLowerCase(),
  name,
  description: null,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
});

const meta = {
  title: 'Content/TagPicker',
  component: TagPicker,
  args: { selected: [], onChange: fn(), canCreate: true, onShowToast: fn() },
} satisfies Meta<typeof TagPicker>;
export default meta;
type Story = StoryObj<typeof meta>;
export const NoTags: Story = {};
export const WithTags: Story = {
  args: { selected: [tag('t1', 'Rust'), tag('t2', 'Docker')] },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText('Rust')).toBeInTheDocument();
    await userEvent.click(canvas.getByRole('button', { name: 'Remove the tag Rust' }));
    await expect(args.onChange).toHaveBeenCalledWith([tag('t2', 'Docker')]);
  },
};
