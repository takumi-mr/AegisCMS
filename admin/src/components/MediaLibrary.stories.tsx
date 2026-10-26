import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { MediaLibrary } from './MediaLibrary';

const meta = {
  title: 'Media/MediaLibrary',
  component: MediaLibrary,
  args: { onSelect: fn(), onClose: fn(), canDelete: false },
} satisfies Meta<typeof MediaLibrary>;
export default meta;
type Story = StoryObj<typeof meta>;
export const SelectMedia: Story = {};
export const CanDelete: Story = { args: { canDelete: true } };
