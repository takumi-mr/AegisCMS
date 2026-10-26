import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fireEvent, within } from 'storybook/test';
import { Avatar } from './Avatar';

const meta = {
  title: 'UI/Avatar',
  component: Avatar,
  parameters: { layout: 'centered' },
  args: { name: 'Alice Cooper', url: null, size: 40 },
} satisfies Meta<typeof Avatar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Initials: Story = {
  args: { name: 'Alice Cooper', url: null },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByText('AC')).toBeVisible();
  },
};

export const SingleName: Story = {
  args: { name: 'Admin', url: null },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByText('AD')).toBeVisible();
  },
};

export const Small: Story = { args: { name: 'John Doe', size: 24 } };

export const Large: Story = { args: { name: 'Takumi Nishida', size: 64 } };

export const WithImage: Story = {
  args: { name: 'Jane Doe', url: '/uploads/sample-avatar.png' },
  play: async ({ canvasElement }) => {
    const image = canvasElement.querySelector('img');
    await expect(image).toBeInTheDocument();
    await expect(image?.src).toContain('sample-avatar.png');
  },
};

export const FailedImageFallsBack: Story = {
  args: { name: 'Ada Lovelace', url: '/missing-avatar.png' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const image = canvasElement.querySelector('img');
    if (!image) throw new Error('Expected avatar image');
    fireEvent.error(image);
    await expect(canvas.findByText('AL')).resolves.toBeVisible();
  },
};
