import type { Meta, StoryObj } from '@storybook/react-vite';
import { Save, Trash2 } from 'lucide-react';
import { expect, fn, userEvent, within } from 'storybook/test';

import { Button } from './Button';

const meta = {
  title: 'UI/Button',
  component: Button,
  args: { onClick: fn(), children: 'Button' },
  parameters: { layout: 'centered' },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Primary: Story = {
  args: {
    variant: 'primary',
    children: (
      <>
        <Save size={16} />
        <span>Save</span>
      </>
    ),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const button = canvas.getByRole('button', { name: 'Save' });
    await userEvent.click(button);
    await expect(args.onClick).toHaveBeenCalledTimes(1);
  },
};

export const Secondary: Story = {
  args: { variant: 'secondary' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const button = canvas.getByRole('button', { name: 'Button' });
    await userEvent.click(button);
    await expect(args.onClick).toHaveBeenCalledTimes(1);
  },
};

export const Tool: Story = { args: { variant: 'tool', children: 'Bold' } };

export const DangerIcon: Story = {
  args: { variant: 'icon', danger: true, 'aria-label': 'Delete', children: <Trash2 size={16} /> },
};

export const Disabled: Story = {
  args: { variant: 'primary', disabled: true },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const button = canvas.getByRole('button');
    await expect(button).toBeDisabled();
    await userEvent.click(button);
    await expect(args.onClick).not.toHaveBeenCalled();
  },
};
