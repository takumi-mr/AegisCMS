import type { Meta, StoryObj } from '@storybook/react-vite';
import { Save, Trash2 } from 'lucide-react';
import { fn } from 'storybook/test';

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
};

export const Secondary: Story = { args: { variant: 'secondary' } };

export const Tool: Story = { args: { variant: 'tool', children: 'Bold' } };

export const DangerIcon: Story = {
  args: { variant: 'icon', danger: true, 'aria-label': 'Delete', children: <Trash2 size={16} /> },
};

export const Disabled: Story = { args: { variant: 'primary', disabled: true } };
