import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, within } from 'storybook/test';
import { Avatar } from './Avatar';

const meta = {
  title: 'UI/Avatar',
  component: Avatar,
  parameters: { layout: 'centered' },
  args: {
    name: 'Alice Cooper',
    url: null,
    size: 40,
  },
} satisfies Meta<typeof Avatar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Initials: Story = {
  args: {
    name: 'Alice Cooper',
    url: null,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const initials = canvas.getByText('AC');
    await expect(initials).toBeVisible();
  },
};

export const SingleName: Story = {
  args: {
    name: 'Admin',
    url: null,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const initials = canvas.getByText('AD');
    await expect(initials).toBeVisible();
  },
};

export const Small: Story = {
  args: {
    name: 'John Doe',
    size: 24,
  },
};

export const Large: Story = {
  args: {
    name: 'Takumi Nishida',
    size: 64,
  },
};

export const WithImage: Story = {
  args: {
    name: 'Jane Doe',
    url: '/uploads/sample-avatar.png',
  },
  play: async ({ canvasElement }) => {
    const img = canvasElement.querySelector('img');
    await expect(img).toBeInTheDocument();
    await expect(img?.src).toContain('sample-avatar.png');
  },
};
