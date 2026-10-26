import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { ImageUploader } from './ImageUploader';

const meta = {
  title: 'Media/ImageUploader',
  component: ImageUploader,
  args: { onUploaded: fn() },
} satisfies Meta<typeof ImageUploader>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
